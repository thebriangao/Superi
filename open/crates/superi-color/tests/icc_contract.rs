use std::sync::Arc;

use superi_color::icc::{
    DisplayProfileCatalog, DisplayProfileDiscovery, DisplayProfileObservation,
    DisplayProfileSnapshot, IccColorSpace, IccProfile, IccProfileClass, IccRenderingIntent,
    MonitorId, NativeDisplayProfileProvider, PresentationProfileState, MAX_ACTIVE_DISPLAYS,
    MAX_ICC_PROFILE_BYTES,
};
use superi_color::view::{MonitorAwareViewport, MonitorAwareViewportState};
use superi_core::error::{ErrorCategory, Result};
use superi_gpu::device::GpuDevice;
use superi_gpu::submission::{GpuFence, GpuSubmissionQueue, GpuSubmissionResources};

#[derive(Clone)]
struct FixedDiscovery {
    observations: Vec<DisplayProfileObservation>,
}

impl DisplayProfileDiscovery for FixedDiscovery {
    fn discover(&self) -> Result<Vec<DisplayProfileObservation>> {
        Ok(self.observations.clone())
    }
}

#[test]
fn display_profiles_are_bounded_validated_and_content_identified() {
    let bytes = profile_bytes(*b"desc", &[0, 0, 0, 0]);
    let profile = IccProfile::parse(bytes.clone()).unwrap();
    assert_eq!(profile.bytes(), bytes.as_slice());
    assert_eq!(profile.version().major(), 4);
    assert_eq!(profile.version().minor(), 4);
    assert_eq!(profile.class(), IccProfileClass::Display);
    assert_eq!(profile.data_color_space(), IccColorSpace::Rgb);
    assert_eq!(profile.connection_space(), IccColorSpace::Xyz);
    assert_eq!(profile.rendering_intent(), IccRenderingIntent::Perceptual);
    assert_eq!(profile.tags().len(), 9);
    assert_eq!(profile.tags()[0].signature(), *b"desc");
    assert_eq!(&profile.tags()[0].data()[0..4], b"mluc");
    assert_eq!(profile.id().to_string().len(), 64);
    assert_eq!(profile, IccProfile::parse(bytes).unwrap());

    let changed = profile_bytes(*b"desc", &[0, 0, 0, 1]);
    assert_ne!(profile.id(), IccProfile::parse(changed).unwrap().id());
}

#[test]
fn malformed_or_non_display_profiles_fail_without_unbounded_allocation() {
    let mut bad_size = profile_bytes(*b"desc", &[0, 0, 0, 0]);
    bad_size[0..4].copy_from_slice(&4_u32.to_be_bytes());
    assert_corrupt(IccProfile::parse(bad_size));

    let mut bad_signature = profile_bytes(*b"desc", &[0, 0, 0, 0]);
    bad_signature[36..40].copy_from_slice(b"nope");
    assert_corrupt(IccProfile::parse(bad_signature));

    let mut input_profile = profile_bytes(*b"desc", &[0, 0, 0, 0]);
    input_profile[12..16].copy_from_slice(b"scnr");
    assert_unsupported(IccProfile::parse(input_profile));

    let mut non_rgb = profile_bytes(*b"desc", &[0, 0, 0, 0]);
    non_rgb[16..20].copy_from_slice(b"CMYK");
    assert_unsupported(IccProfile::parse(non_rgb));

    let mut out_of_bounds = profile_bytes(*b"desc", &[0, 0, 0, 0]);
    out_of_bounds[140..144].copy_from_slice(&u32::MAX.to_be_bytes());
    assert_corrupt(IccProfile::parse(out_of_bounds));

    let duplicate_tags = profile_with_tags(&[
        (*b"desc", typed_tag(*b"mluc", &[0, 0, 0, 0])),
        (*b"desc", typed_tag(*b"mluc", &[1, 2, 3, 4])),
    ]);
    assert_corrupt(IccProfile::parse(duplicate_tags));

    let too_large = vec![0_u8; MAX_ICC_PROFILE_BYTES + 1];
    let error = IccProfile::parse(too_large).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
}

#[test]
fn icc2022_header_and_tag_element_layout_are_enforced() {
    let valid = profile_bytes(*b"desc", &[4, 4, 0, 0]);
    IccProfile::parse(valid.clone()).unwrap();

    let mut reserved_header = valid.clone();
    reserved_header[100] = 1;
    assert_corrupt(IccProfile::parse(reserved_header));

    let mut unaligned_profile_size = valid.clone();
    unaligned_profile_size.push(0);
    set_profile_size(&mut unaligned_profile_size);
    assert_corrupt(IccProfile::parse(unaligned_profile_size));

    let mut gap_before_data = valid.clone();
    insert_gap_before_tag_data(&mut gap_before_data);
    assert_corrupt(IccProfile::parse(gap_before_data));

    let mut nonzero_padding = valid.clone();
    let (trc_offset, trc_size) = tag_range(&nonzero_padding, *b"rTRC");
    assert_ne!(trc_size % 4, 0);
    nonzero_padding[trc_offset + trc_size] = 1;
    assert_corrupt(IccProfile::parse(nonzero_padding));

    let mut partial_overlap = valid;
    let (first_offset, _) = tag_range(&partial_overlap, *b"desc");
    set_tag_offset(&mut partial_overlap, *b"cprt", first_offset + 4);
    assert_corrupt(IccProfile::parse(partial_overlap));

    IccProfile::parse(profile_with_shared_desc_and_copyright()).unwrap();
}

#[test]
fn icc2022_required_display_tags_and_types_are_enforced() {
    let missing_common = profile_with_tags(
        &matrix_tags(&[8, 8, 8, 8])
            .into_iter()
            .filter(|(signature, _)| signature != b"cprt")
            .collect::<Vec<_>>(),
    );
    assert_corrupt(IccProfile::parse(missing_common));

    let mut wrong_white_type = profile_bytes(*b"desc", &[8, 8, 8, 8]);
    let (white_offset, _) = tag_range(&wrong_white_type, *b"wtpt");
    wrong_white_type[white_offset..white_offset + 4].copy_from_slice(b"curv");
    assert_corrupt(IccProfile::parse(wrong_white_type));

    let missing_matrix_member = profile_with_tags(
        &matrix_tags(&[8, 8, 8, 8])
            .into_iter()
            .filter(|(signature, _)| signature != b"bTRC")
            .collect::<Vec<_>>(),
    );
    assert_corrupt(IccProfile::parse(missing_matrix_member));

    let incomplete_lut = profile_with_tags(&lut_tags(false));
    assert_corrupt(IccProfile::parse(incomplete_lut));
    IccProfile::parse(profile_with_tags(&lut_tags(true))).unwrap();
}

#[test]
fn refresh_is_deterministic_atomic_and_exposes_exact_changes() {
    let left = observation(
        "display:z",
        "Reference",
        false,
        false,
        Some(profile_bytes(*b"desc", &[0, 0, 0, 1])),
    );
    let right = observation(
        "display:a",
        "Primary",
        true,
        true,
        Some(profile_bytes(*b"desc", &[0, 0, 0, 2])),
    );
    let mut catalog = DisplayProfileCatalog::new();

    let first = catalog
        .refresh(&FixedDiscovery {
            observations: vec![left.clone(), right.clone()],
        })
        .unwrap();
    assert!(first.changed());
    assert_eq!(first.generation(), 1);
    assert_eq!(
        first.added(),
        &[
            MonitorId::new("display:a").unwrap(),
            MonitorId::new("display:z").unwrap(),
        ]
    );
    assert!(first.removed().is_empty());
    assert!(first.profile_changed().is_empty());

    let snapshot = catalog.snapshot();
    assert_eq!(snapshot.generation(), 1);
    assert_eq!(snapshot.displays()[0].id().as_str(), "display:a");
    assert_eq!(snapshot.displays()[1].id().as_str(), "display:z");
    assert_eq!(snapshot.primary_display().unwrap().name(), "Primary");

    let unchanged = catalog
        .refresh(&FixedDiscovery {
            observations: vec![right.clone(), left.clone()],
        })
        .unwrap();
    assert!(!unchanged.changed());
    assert_eq!(unchanged.generation(), 1);

    let prior = catalog.snapshot();
    let duplicate = catalog.refresh(&FixedDiscovery {
        observations: vec![right.clone(), right],
    });
    let error = duplicate.unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(catalog.snapshot(), prior);

    let mut invalid_bytes = profile_bytes(*b"desc", &[0, 0, 0, 3]);
    invalid_bytes[36..40].copy_from_slice(b"nope");
    let invalid = catalog.refresh(&FixedDiscovery {
        observations: vec![observation(
            "display:a",
            "Primary",
            true,
            true,
            Some(invalid_bytes),
        )],
    });
    assert_eq!(invalid.unwrap_err().category(), ErrorCategory::CorruptData);
    assert_eq!(catalog.snapshot(), prior);

    let changed_profile = observation(
        "display:a",
        "Primary",
        true,
        true,
        Some(profile_bytes(*b"desc", &[0, 0, 0, 9])),
    );
    let changed = catalog
        .refresh(&FixedDiscovery {
            observations: vec![changed_profile],
        })
        .unwrap();
    assert_eq!(changed.generation(), 2);
    assert_eq!(changed.removed(), &[MonitorId::new("display:z").unwrap()]);
    assert_eq!(
        changed.profile_changed(),
        &[MonitorId::new("display:a").unwrap()]
    );
}

#[test]
fn monitor_aware_bindings_switch_reversibly_and_never_hide_stale_state() {
    let primary = observation(
        "display:primary",
        "Primary",
        true,
        true,
        Some(profile_bytes(*b"desc", &[0, 0, 0, 1])),
    );
    let unprofiled = observation("display:projector", "Projector", false, false, None);
    let mut catalog = DisplayProfileCatalog::new();
    catalog
        .refresh(&FixedDiscovery {
            observations: vec![primary.clone(), unprofiled.clone()],
        })
        .unwrap();

    let primary_id = MonitorId::new("display:primary").unwrap();
    let projector_id = MonitorId::new("display:projector").unwrap();
    let first = catalog.bind_for_presentation(&primary_id).unwrap();
    assert!(matches!(
        first.state(),
        PresentationProfileState::Profiled { .. }
    ));
    assert!(first.is_current(&catalog.snapshot()));

    let projector = catalog.bind_for_presentation(&projector_id).unwrap();
    assert_eq!(projector.state(), &PresentationProfileState::Unprofiled);
    assert!(projector.is_current(&catalog.snapshot()));

    let restored = catalog.bind_for_presentation(&primary_id).unwrap();
    assert_eq!(restored.profile_id(), first.profile_id());

    catalog
        .refresh(&FixedDiscovery {
            observations: vec![
                observation(
                    "display:primary",
                    "Primary",
                    true,
                    true,
                    Some(profile_bytes(*b"desc", &[0, 0, 0, 7])),
                ),
                unprofiled,
            ],
        })
        .unwrap();
    assert!(!first.is_current(&catalog.snapshot()));
    assert!(catalog
        .bind_for_presentation(&primary_id)
        .unwrap()
        .is_current(&catalog.snapshot()));

    let error = catalog
        .bind_for_presentation(&MonitorId::new("display:missing").unwrap())
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::NotFound);
}

#[test]
fn monitor_aware_viewport_state_guards_the_real_presentation_path() {
    let primary_id = MonitorId::new("display:primary").unwrap();
    let projector_id = MonitorId::new("display:projector").unwrap();
    let mut catalog = DisplayProfileCatalog::new();
    catalog
        .refresh(&FixedDiscovery {
            observations: vec![
                observation(
                    primary_id.as_str(),
                    "Primary",
                    true,
                    true,
                    Some(profile_bytes(*b"desc", &[1, 0, 0, 0])),
                ),
                observation(projector_id.as_str(), "Projector", false, false, None),
            ],
        })
        .unwrap();

    let mut state = MonitorAwareViewportState::new(&catalog.snapshot(), &primary_id).unwrap();
    let acquired = state.frame_token(&catalog.snapshot(), &primary_id).unwrap();
    assert_eq!(acquired.monitor_id(), &primary_id);
    assert_eq!(
        acquired
            .ensure_current_on(&catalog.snapshot(), &projector_id)
            .unwrap_err()
            .category(),
        ErrorCategory::Conflict
    );

    catalog
        .refresh(&FixedDiscovery {
            observations: vec![
                observation(
                    primary_id.as_str(),
                    "Primary",
                    true,
                    true,
                    Some(profile_bytes(*b"desc", &[9, 0, 0, 0])),
                ),
                observation(projector_id.as_str(), "Projector", false, false, None),
            ],
        })
        .unwrap();
    let refreshed = catalog.snapshot();
    assert_eq!(
        acquired.ensure_current(&refreshed).unwrap_err().category(),
        ErrorCategory::Conflict
    );
    assert!(state.refresh_profile(&refreshed).unwrap().changed());

    let moved = state.move_to_monitor(&refreshed, &projector_id).unwrap();
    assert!(moved.changed());
    assert_eq!(
        state.binding().state(),
        &PresentationProfileState::Unprofiled
    );
    let restored = state.move_to_monitor(&refreshed, &primary_id).unwrap();
    assert!(restored.changed());
    assert_eq!(state.binding().monitor_id(), &primary_id);

    fn real_surface_is_consumed_by_color_owner<'device>(
        viewport: &mut MonitorAwareViewport,
        snapshot: &DisplayProfileSnapshot,
        monitor_id: &MonitorId,
        device: &'device GpuDevice,
        submissions: &GpuSubmissionQueue<'device>,
        command_buffer: superi_gpu::wgpu::CommandBuffer,
        retained: GpuSubmissionResources<'device>,
    ) -> Result<GpuFence> {
        let frame = viewport.acquire_frame(snapshot, monitor_id, device)?;
        let _texture = frame.texture();
        let _profile = frame.presentation_token().profile_state();
        frame.submit_and_present(
            snapshot,
            monitor_id,
            submissions,
            [command_buffer],
            retained,
        )
    }
    let _ = real_surface_is_consumed_by_color_owner;
}

#[test]
fn catalog_bounds_display_count_and_requires_unambiguous_primary_state() {
    let mut catalog = DisplayProfileCatalog::new();
    let too_many = (0..=MAX_ACTIVE_DISPLAYS)
        .map(|index| {
            observation(
                &format!("display:{index:03}"),
                &format!("Display {index}"),
                index == 0,
                false,
                None,
            )
        })
        .collect();
    let error = catalog
        .refresh(&FixedDiscovery {
            observations: too_many,
        })
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
    assert_eq!(catalog.snapshot().generation(), 0);

    let error = catalog
        .refresh(&FixedDiscovery {
            observations: vec![
                observation("display:a", "A", true, false, None),
                observation("display:b", "B", true, false, None),
            ],
        })
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(catalog.snapshot().generation(), 0);
}

#[test]
fn native_shell_provider_is_consumable_on_every_desktop_target() {
    let observations = vec![
        observation(
            "windows-device:\\\\.\\DISPLAY1",
            "Windows Display 1",
            true,
            false,
            Some(profile_bytes(*b"desc", &[1, 0, 0, 0])),
        ),
        observation("linux-x11-crtc:42", "X11 CRTC 42", false, false, None),
        observation(
            "linux-wayland-output:7",
            "Wayland Output 7",
            false,
            true,
            Some(profile_bytes(*b"desc", &[2, 0, 0, 0])),
        ),
    ];
    let provider = NativeDisplayProfileProvider::new(observations.clone()).unwrap();
    assert_eq!(provider.observations(), observations.as_slice());

    let mut catalog = DisplayProfileCatalog::new();
    let update = catalog.refresh(&provider).unwrap();
    assert!(update.changed());
    assert_eq!(catalog.snapshot().displays().len(), 3);
    assert!(catalog
        .snapshot()
        .display(&MonitorId::new("linux-x11-crtc:42").unwrap())
        .unwrap()
        .profile()
        .is_none());

    let too_many = (0..=MAX_ACTIVE_DISPLAYS)
        .map(|index| observation(&format!("native:{index}"), "Display", false, false, None))
        .collect();
    assert_eq!(
        NativeDisplayProfileProvider::new(too_many)
            .unwrap_err()
            .category(),
        ErrorCategory::ResourceExhausted
    );
}

#[test]
fn profile_state_is_immutable_and_safe_to_share() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<IccProfile>();
    assert_send_sync::<DisplayProfileCatalog>();

    let profile = IccProfile::parse(profile_bytes(*b"desc", &[1, 2, 3, 4])).unwrap();
    let clone = profile.clone();
    let thread = std::thread::spawn(move || clone.id());
    assert_eq!(thread.join().unwrap(), profile.id());
}

#[test]
fn core_graphics_discovery_is_listed_in_the_unsafe_boundary_inventory() {
    let inventory = include_str!("../../../../docs/unsafe-ffi.md");
    assert!(inventory.contains("macOS CoreGraphics display profile discovery"));
    assert!(inventory.contains("open/crates/superi-color/src/icc/macos.rs"));
    assert!(inventory.contains("CGGetActiveDisplayList"));
}

#[cfg(target_os = "macos")]
#[test]
fn macos_system_discovery_reports_active_displays_without_guessing_profiles() {
    use superi_color::icc::SystemDisplayProfileDiscovery;

    let observations = match SystemDisplayProfileDiscovery.discover() {
        Ok(observations) => observations,
        Err(error) => {
            assert_eq!(error.category(), ErrorCategory::Unavailable);
            assert_eq!(error.contexts()[0].component(), "superi-color.icc");
            return;
        }
    };
    assert!(!observations.is_empty());
    assert_eq!(
        observations.iter().filter(|item| item.is_primary()).count(),
        1
    );
    for observation in observations {
        assert!(observation.id().as_str().starts_with("macos-cgdisplay:"));
        if let Some(bytes) = observation.icc_profile_bytes() {
            IccProfile::parse(Arc::<[u8]>::from(bytes)).unwrap();
        }
    }
}

fn observation(
    id: &str,
    name: &str,
    primary: bool,
    built_in: bool,
    profile: Option<Vec<u8>>,
) -> DisplayProfileObservation {
    DisplayProfileObservation::new(
        MonitorId::new(id).unwrap(),
        name,
        primary,
        built_in,
        profile.map(Arc::<[u8]>::from),
    )
    .unwrap()
}

fn profile_bytes(signature: [u8; 4], payload: &[u8]) -> Vec<u8> {
    assert_eq!(signature, *b"desc");
    profile_with_tags(&matrix_tags(payload))
}

fn matrix_tags(seed: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
    vec![
        (*b"desc", typed_tag(*b"mluc", seed)),
        (*b"cprt", typed_tag(*b"mluc", b"Superi")),
        (*b"wtpt", xyz_tag()),
        (*b"rXYZ", xyz_tag()),
        (*b"gXYZ", xyz_tag()),
        (*b"bXYZ", xyz_tag()),
        (*b"rTRC", curve_tag()),
        (*b"gTRC", curve_tag()),
        (*b"bTRC", curve_tag()),
    ]
}

fn lut_tags(complete: bool) -> Vec<([u8; 4], Vec<u8>)> {
    let mut tags = vec![
        (*b"desc", typed_tag(*b"mluc", b"LUT display")),
        (*b"cprt", typed_tag(*b"mluc", b"Superi")),
        (*b"wtpt", xyz_tag()),
        (*b"A2B0", typed_tag(*b"mAB ", &[0; 24])),
    ];
    if complete {
        tags.push((*b"B2A0", typed_tag(*b"mBA ", &[0; 24])));
    }
    tags
}

fn typed_tag(signature: [u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut data = Vec::with_capacity(8 + payload.len());
    data.extend_from_slice(&signature);
    data.extend_from_slice(&[0; 4]);
    data.extend_from_slice(payload);
    data
}

fn xyz_tag() -> Vec<u8> {
    let mut values = Vec::with_capacity(12);
    values.extend_from_slice(&0x0000_f6d6_u32.to_be_bytes());
    values.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
    values.extend_from_slice(&0x0000_d32d_u32.to_be_bytes());
    typed_tag(*b"XYZ ", &values)
}

fn curve_tag() -> Vec<u8> {
    let mut payload = Vec::with_capacity(6);
    payload.extend_from_slice(&1_u32.to_be_bytes());
    payload.extend_from_slice(&0x0100_u16.to_be_bytes());
    typed_tag(*b"curv", &payload)
}

fn profile_with_tags(tags: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let table_size = 4 + tags.len() * 12;
    let mut next_offset = 128 + table_size;
    let mut bytes = vec![0_u8; next_offset];
    bytes[8] = 0x04;
    bytes[9] = 0x40;
    bytes[12..16].copy_from_slice(b"mntr");
    bytes[16..20].copy_from_slice(b"RGB ");
    bytes[20..24].copy_from_slice(b"XYZ ");
    bytes[36..40].copy_from_slice(b"acsp");
    bytes[64..68].copy_from_slice(&0_u32.to_be_bytes());
    bytes[68..72].copy_from_slice(&0x0000_f6d6_u32.to_be_bytes());
    bytes[72..76].copy_from_slice(&0x0001_0000_u32.to_be_bytes());
    bytes[76..80].copy_from_slice(&0x0000_d32d_u32.to_be_bytes());
    bytes[128..132].copy_from_slice(&(tags.len() as u32).to_be_bytes());

    for (index, (signature, payload)) in tags.iter().enumerate() {
        while next_offset % 4 != 0 {
            bytes.push(0);
            next_offset += 1;
        }
        let entry = 132 + index * 12;
        bytes[entry..entry + 4].copy_from_slice(signature);
        bytes[entry + 4..entry + 8].copy_from_slice(&(next_offset as u32).to_be_bytes());
        bytes[entry + 8..entry + 12].copy_from_slice(&(payload.len() as u32).to_be_bytes());
        bytes.extend_from_slice(payload);
        next_offset += payload.len();
    }
    while bytes.len() % 4 != 0 {
        bytes.push(0);
    }
    set_profile_size(&mut bytes);
    bytes
}

fn set_profile_size(bytes: &mut [u8]) {
    let size = bytes.len() as u32;
    bytes[0..4].copy_from_slice(&size.to_be_bytes());
}

fn tag_entry(bytes: &[u8], signature: [u8; 4]) -> usize {
    let count = u32::from_be_bytes(bytes[128..132].try_into().unwrap()) as usize;
    (0..count)
        .map(|index| 132 + index * 12)
        .find(|entry| bytes[*entry..*entry + 4] == signature)
        .unwrap()
}

fn tag_range(bytes: &[u8], signature: [u8; 4]) -> (usize, usize) {
    let entry = tag_entry(bytes, signature);
    let offset = u32::from_be_bytes(bytes[entry + 4..entry + 8].try_into().unwrap()) as usize;
    let size = u32::from_be_bytes(bytes[entry + 8..entry + 12].try_into().unwrap()) as usize;
    (offset, size)
}

fn set_tag_offset(bytes: &mut [u8], signature: [u8; 4], offset: usize) {
    let entry = tag_entry(bytes, signature);
    bytes[entry + 4..entry + 8].copy_from_slice(&(offset as u32).to_be_bytes());
}

fn insert_gap_before_tag_data(bytes: &mut Vec<u8>) {
    let count = u32::from_be_bytes(bytes[128..132].try_into().unwrap()) as usize;
    let table_end = 132 + count * 12;
    bytes.splice(table_end..table_end, [0; 4]);
    for index in 0..count {
        let entry = 132 + index * 12;
        let offset = u32::from_be_bytes(bytes[entry + 4..entry + 8].try_into().unwrap());
        bytes[entry + 4..entry + 8].copy_from_slice(&(offset + 4).to_be_bytes());
    }
    set_profile_size(bytes);
}

fn profile_with_shared_desc_and_copyright() -> Vec<u8> {
    let mut bytes = profile_bytes(*b"desc", b"Superi");
    let (description_offset, description_size) = tag_range(&bytes, *b"desc");
    let (copyright_offset, copyright_size) = tag_range(&bytes, *b"cprt");
    assert_eq!(description_size, copyright_size);
    let padded_size = (copyright_size + 3) & !3;
    set_tag_offset(&mut bytes, *b"cprt", description_offset);
    let copyright_entry = tag_entry(&bytes, *b"cprt");
    bytes[copyright_entry + 8..copyright_entry + 12]
        .copy_from_slice(&(description_size as u32).to_be_bytes());
    bytes.drain(copyright_offset..copyright_offset + padded_size);

    let count = u32::from_be_bytes(bytes[128..132].try_into().unwrap()) as usize;
    for index in 0..count {
        let entry = 132 + index * 12;
        let offset = u32::from_be_bytes(bytes[entry + 4..entry + 8].try_into().unwrap()) as usize;
        if offset > copyright_offset {
            bytes[entry + 4..entry + 8]
                .copy_from_slice(&((offset - padded_size) as u32).to_be_bytes());
        }
    }
    set_profile_size(&mut bytes);
    bytes
}

fn assert_corrupt(result: Result<IccProfile>) {
    let error = result.unwrap_err();
    assert_eq!(error.category(), ErrorCategory::CorruptData);
    assert!(!error.contexts().is_empty());
}

fn assert_unsupported(result: Result<IccProfile>) {
    let error = result.unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unsupported);
    assert!(!error.contexts().is_empty());
}

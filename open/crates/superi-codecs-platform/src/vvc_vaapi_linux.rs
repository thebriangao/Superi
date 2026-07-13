//! Raw VA-API 1.22 VVC decode session for Linux.

#![allow(unsafe_code)]

use std::collections::BTreeSet;
use std::ffi::CStr;
use std::fs::{File, OpenOptions};
use std::mem::size_of;
use std::os::fd::{AsRawFd, FromRawFd};
use std::path::Path;
use std::ptr;
use std::sync::Arc;

use oxideav_h266::slice_header::SliceType;
use oxideav_h266::{aps::AdaptationParameterSet, aps::ApsParamsType};

use super::vvc::{ParsedVvcPicture, VvcBitstreamParser};

#[allow(
    clippy::all,
    clippy::undocumented_unsafe_blocks,
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    improper_ctypes,
    unused_imports,
    unsafe_op_in_unsafe_fn
)]
mod ffi {
    include!(concat!(env!("OUT_DIR"), "/libva_vvc.rs"));
}

const MAX_EXPORTED_OBJECTS: usize = 4;
const MAX_EXPORTED_PLANES: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VvcPlaneLayout {
    pub(crate) object_index: usize,
    pub(crate) offset: u32,
    pub(crate) pitch: u32,
}

#[derive(Debug)]
pub(crate) struct VvcVaapiFrame {
    pub(crate) dma_handles: Vec<File>,
    pub(crate) modifier: u64,
    pub(crate) planes: Vec<VvcPlaneLayout>,
}

impl VvcVaapiFrame {
    pub(crate) fn object_count(&self) -> usize {
        self.dma_handles.len()
    }

    pub(crate) fn plane_count(&self) -> usize {
        self.planes.len()
    }

    pub(crate) fn modifier(&self) -> u64 {
        self.modifier
    }
}

pub(crate) struct VvcDecodedFrame {
    pub(crate) token: u64,
    pub(crate) frame: Arc<VvcVaapiFrame>,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

pub(crate) struct VvcVaapiDecoder {
    display: ffi::VADisplay,
    config: ffi::VAConfigID,
    context: ffi::VAContextID,
    context_size: Option<(u32, u32)>,
    parser: VvcBitstreamParser,
    _render_node: File,
}

impl VvcVaapiDecoder {
    pub(crate) fn new(render_node: &Path) -> Result<Self, String> {
        let render_node = OpenOptions::new()
            .read(true)
            .write(true)
            .open(render_node)
            .map_err(|error| format!("open VA render node: {error}"))?;
        // SAFETY: The file descriptor is open for the complete display lifetime held by Self.
        let display = unsafe { ffi::vaGetDisplayDRM(render_node.as_raw_fd()) };
        if display.is_null() {
            return Err("vaGetDisplayDRM returned a null display".to_owned());
        }
        let mut major = 0;
        let mut minor = 0;
        // SAFETY: The display is nonnull and both version outputs are valid writable integers.
        if let Err(error) = va_status(unsafe { ffi::vaInitialize(display, &mut major, &mut minor) })
        {
            return Err(format!("initialize VA display: {error}"));
        }
        if major < 1 || (major == 1 && minor < 22) {
            // SAFETY: Initialization succeeded and no dependent VA object has been created.
            unsafe {
                ffi::vaTerminate(display);
            }
            return Err(format!(
                "VVC decode requires VA-API 1.22 or newer, driver exposes {major}.{minor}"
            ));
        }

        let mut attribute = ffi::VAConfigAttrib {
            type_: ffi::VAConfigAttribType::VAConfigAttribRTFormat,
            value: 0,
        };
        // SAFETY: The initialized display and one-element output attribute remain live for the call.
        let attribute_status = unsafe {
            ffi::vaGetConfigAttributes(
                display,
                ffi::VAProfile::VAProfileVVCMain10,
                ffi::VAEntrypoint::VAEntrypointVLD,
                &mut attribute,
                1,
            )
        };
        if let Err(error) = va_status(attribute_status) {
            // SAFETY: No dependent VA object was created after the successful initialization.
            unsafe {
                ffi::vaTerminate(display);
            }
            return Err(format!("query VVC Main 10 attributes: {error}"));
        }
        if attribute.value == ffi::VA_ATTRIB_NOT_SUPPORTED
            || attribute.value & ffi::VA_RT_FORMAT_YUV420_10 == 0
        {
            // SAFETY: No dependent VA object was created after the successful initialization.
            unsafe {
                ffi::vaTerminate(display);
            }
            return Err("VA driver does not expose VVC Main 10 P010 decode".to_owned());
        }

        attribute.value = ffi::VA_RT_FORMAT_YUV420_10;
        let mut config = ffi::VA_INVALID_ID;
        // SAFETY: The display is initialized, and the one-element attribute and config output are valid.
        let config_status = unsafe {
            ffi::vaCreateConfig(
                display,
                ffi::VAProfile::VAProfileVVCMain10,
                ffi::VAEntrypoint::VAEntrypointVLD,
                &mut attribute,
                1,
                &mut config,
            )
        };
        if let Err(error) = va_status(config_status) {
            // SAFETY: Configuration creation failed, so only the initialized display needs release.
            unsafe {
                ffi::vaTerminate(display);
            }
            return Err(format!("create VVC Main 10 configuration: {error}"));
        }
        let decoder = Self {
            display,
            config,
            context: ffi::VA_INVALID_ID,
            context_size: None,
            parser: VvcBitstreamParser::default(),
            _render_node: render_node,
        };
        let mut probe_surface = decoder.create_surface(64, 64)?;
        // SAFETY: The probe surface was created by this display and has no dependent objects.
        va_status(unsafe { ffi::vaDestroySurfaces(decoder.display, &mut probe_surface, 1) })
            .map_err(|error| format!("destroy P010 VVC probe surface: {error}"))?;
        Ok(decoder)
    }

    pub(crate) fn decode(
        &mut self,
        token: u64,
        data: &[u8],
    ) -> Result<Vec<VvcDecodedFrame>, String> {
        let Some(picture) = self.parser.parse_access_unit(token, data)? else {
            return Ok(Vec::new());
        };
        validate_supported_picture(&picture)?;
        self.ensure_context(picture.width, picture.height)?;
        let surface = self.create_surface(picture.width, picture.height)?;
        let result = self.submit_picture(&picture, surface).and_then(|()| {
            // SAFETY: The surface belongs to this display and picture submission has completed.
            va_status(unsafe { ffi::vaSyncSurface(self.display, surface) })
                .map_err(|error| format!("synchronize VVC output surface: {error}"))?;
            self.export_surface(surface, picture.width, picture.height)
        });
        // SAFETY: The surface is owned by this display and is destroyed exactly once after export.
        unsafe {
            ffi::vaDestroySurfaces(self.display, &mut [surface][0], 1);
        }
        let frame = result?;
        Ok(vec![VvcDecodedFrame {
            token: picture.token,
            frame: Arc::new(frame),
            width: picture.width,
            height: picture.height,
        }])
    }

    pub(crate) fn flush(&mut self) -> Result<Vec<VvcDecodedFrame>, String> {
        Ok(Vec::new())
    }

    fn ensure_context(&mut self, width: u32, height: u32) -> Result<(), String> {
        if self.context_size == Some((width, height)) {
            return Ok(());
        }
        if self.context != ffi::VA_INVALID_ID {
            // SAFETY: The context belongs to this display and no submission is in progress.
            va_status(unsafe { ffi::vaDestroyContext(self.display, self.context) })
                .map_err(|error| format!("destroy prior VVC context: {error}"))?;
            self.context = ffi::VA_INVALID_ID;
        }
        let width_i32 = i32::try_from(width)
            .map_err(|_| "VVC width exceeds the VA context domain".to_owned())?;
        let height_i32 = i32::try_from(height)
            .map_err(|_| "VVC height exceeds the VA context domain".to_owned())?;
        // SAFETY: The configuration belongs to this display and every output pointer is valid.
        let status = unsafe {
            ffi::vaCreateContext(
                self.display,
                self.config,
                width_i32,
                height_i32,
                ffi::VA_PROGRESSIVE as i32,
                ptr::null_mut(),
                0,
                &mut self.context,
            )
        };
        va_status(status).map_err(|error| format!("create VVC decode context: {error}"))?;
        self.context_size = Some((width, height));
        Ok(())
    }

    fn create_surface(&self, width: u32, height: u32) -> Result<ffi::VASurfaceID, String> {
        let mut integer = ffi::_VAGenericValue__bindgen_ty_1::default();
        integer.i = ffi::VA_FOURCC_P010 as i32;
        let value = ffi::VAGenericValue {
            type_: ffi::VAGenericValueType::VAGenericValueTypeInteger,
            value: integer,
        };
        let mut attribute = ffi::VASurfaceAttrib {
            type_: ffi::VASurfaceAttribType::VASurfaceAttribPixelFormat,
            flags: ffi::VA_SURFACE_ATTRIB_SETTABLE,
            value,
        };
        let mut surface = ffi::VA_INVALID_SURFACE;
        // SAFETY: The attribute and surface output are valid for this initialized display.
        let status = unsafe {
            ffi::vaCreateSurfaces(
                self.display,
                ffi::VA_RT_FORMAT_YUV420_10,
                width,
                height,
                &mut surface,
                1,
                &mut attribute,
                1,
            )
        };
        va_status(status).map_err(|error| format!("create P010 VVC surface: {error}"))?;
        Ok(surface)
    }

    fn submit_picture(
        &self,
        picture: &ParsedVvcPicture,
        surface: ffi::VASurfaceID,
    ) -> Result<(), String> {
        let mut picture_parameters = map_picture_parameters(picture, surface)?;
        let mut slice_parameters = map_slice_parameters(picture)?;
        let slice = &picture.slices[0];
        let mut buffers = Vec::with_capacity(3 + picture.aps.len());
        let picture_buffer = self.create_buffer(
            ffi::VABufferType::VAPictureParameterBufferType,
            &mut picture_parameters,
        )?;
        buffers.push(picture_buffer);
        let slice_buffer = match self.create_buffer(
            ffi::VABufferType::VASliceParameterBufferType,
            &mut slice_parameters,
        ) {
            Ok(buffer) => buffer,
            Err(error) => {
                self.destroy_buffers(&buffers);
                return Err(error);
            }
        };
        buffers.push(slice_buffer);
        for aps in &picture.aps {
            let buffer = match aps.aps_params_type {
                ApsParamsType::Alf => map_alf_parameters(aps).and_then(|mut parameters| {
                    self.create_buffer(ffi::VABufferType::VAAlfBufferType, &mut parameters)
                }),
                ApsParamsType::Lmcs => map_lmcs_parameters(aps).and_then(|mut parameters| {
                    self.create_buffer(ffi::VABufferType::VALmcsBufferType, &mut parameters)
                }),
                ApsParamsType::Scaling | ApsParamsType::Reserved(_) => continue,
            };
            let buffer = match buffer {
                Ok(buffer) => buffer,
                Err(error) => {
                    self.destroy_buffers(&buffers);
                    return Err(error);
                }
            };
            buffers.push(buffer);
        }
        let data_buffer =
            match self.create_bytes_buffer(ffi::VABufferType::VASliceDataBufferType, &slice.nal) {
                Ok(buffer) => buffer,
                Err(error) => {
                    self.destroy_buffers(&buffers);
                    return Err(error);
                }
            };
        buffers.push(data_buffer);

        let submission = (|| {
            // SAFETY: Context and surface are live, compatible, and confined to this worker thread.
            va_status(unsafe { ffi::vaBeginPicture(self.display, self.context, surface) })
                .map_err(|error| format!("begin VVC picture: {error}"))?;
            // SAFETY: The buffer identifier slice remains live and each identifier belongs to context.
            if let Err(error) = va_status(unsafe {
                ffi::vaRenderPicture(
                    self.display,
                    self.context,
                    buffers.as_mut_ptr(),
                    i32::try_from(buffers.len()).unwrap_or(i32::MAX),
                )
            }) {
                // SAFETY: A matching begin call succeeded, so ending unwinds the failed submission.
                unsafe {
                    ffi::vaEndPicture(self.display, self.context);
                }
                return Err(format!("render VVC picture: {error}"));
            }
            // SAFETY: A matching begin call succeeded and render accepted every submitted buffer.
            va_status(unsafe { ffi::vaEndPicture(self.display, self.context) })
                .map_err(|error| format!("end VVC picture: {error}"))
        })();
        self.destroy_buffers(&buffers);
        submission
    }

    fn destroy_buffers(&self, buffers: &[ffi::VABufferID]) {
        for &buffer in buffers {
            // SAFETY: Each buffer belongs to this display and each call receives one live identifier.
            unsafe {
                ffi::vaDestroyBuffer(self.display, buffer);
            }
        }
    }

    fn create_buffer<T>(
        &self,
        buffer_type: ffi::VABufferType::Type,
        value: &mut T,
    ) -> Result<ffi::VABufferID, String> {
        let size = u32::try_from(size_of::<T>())
            .map_err(|_| "VA parameter buffer size overflowed".to_owned())?;
        let mut id = ffi::VA_INVALID_ID;
        // SAFETY: T is a live generated VA parameter struct and size exactly matches its allocation.
        let status = unsafe {
            ffi::vaCreateBuffer(
                self.display,
                self.context,
                buffer_type,
                size,
                1,
                ptr::from_mut(value).cast(),
                &mut id,
            )
        };
        va_status(status).map_err(|error| format!("create VVC parameter buffer: {error}"))?;
        Ok(id)
    }

    fn create_bytes_buffer(
        &self,
        buffer_type: ffi::VABufferType::Type,
        value: &[u8],
    ) -> Result<ffi::VABufferID, String> {
        let size = u32::try_from(value.len())
            .map_err(|_| "VVC slice data exceeds the VA buffer domain".to_owned())?;
        let mut id = ffi::VA_INVALID_ID;
        // SAFETY: The immutable byte slice remains live for the call and its checked length is exact.
        let status = unsafe {
            ffi::vaCreateBuffer(
                self.display,
                self.context,
                buffer_type,
                size,
                1,
                value.as_ptr().cast_mut().cast(),
                &mut id,
            )
        };
        va_status(status).map_err(|error| format!("create VVC slice buffer: {error}"))?;
        Ok(id)
    }

    fn export_surface(
        &self,
        surface: ffi::VASurfaceID,
        width: u32,
        height: u32,
    ) -> Result<VvcVaapiFrame, String> {
        let mut descriptor = ffi::VADRMPRIMESurfaceDescriptor::default();
        // SAFETY: The surface is synchronized by the caller and descriptor is valid writable storage.
        let status = unsafe {
            ffi::vaExportSurfaceHandle(
                self.display,
                surface,
                ffi::VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2,
                ffi::VA_EXPORT_SURFACE_READ_ONLY | ffi::VA_EXPORT_SURFACE_COMPOSED_LAYERS,
                ptr::from_mut(&mut descriptor).cast(),
            )
        };
        va_status(status).map_err(|error| format!("export VVC P010 surface: {error}"))?;
        let object_count = usize::try_from(descriptor.num_objects)
            .map_err(|_| "VA exported an invalid DMA-BUF object count".to_owned())?;
        let layer_count = usize::try_from(descriptor.num_layers)
            .map_err(|_| "VA exported an invalid DMA-BUF layer count".to_owned())?;
        if object_count == 0
            || object_count > MAX_EXPORTED_OBJECTS
            || layer_count != 1
            || descriptor.fourcc != ffi::VA_FOURCC_P010
            || descriptor.width != width
            || descriptor.height != height
        {
            close_exported_fds(&descriptor);
            return Err("VA driver exported an unsupported VVC surface layout".to_owned());
        }
        let layer = descriptor.layers[0];
        let plane_count = usize::try_from(layer.num_planes)
            .map_err(|_| "VA exported an invalid plane count".to_owned())?;
        if plane_count != 2
            || plane_count > MAX_EXPORTED_PLANES
            || layer.drm_format != ffi::VA_FOURCC_P010
        {
            close_exported_fds(&descriptor);
            return Err("VA driver did not export a two-plane P010 surface".to_owned());
        }
        let mut planes = Vec::with_capacity(plane_count);
        for index in 0..plane_count {
            let object_index = usize::try_from(layer.object_index[index])
                .map_err(|_| "VA exported an invalid object index".to_owned())?;
            if object_index >= object_count {
                close_exported_fds(&descriptor);
                return Err("VA exported a plane outside its DMA-BUF object table".to_owned());
            }
            let pitch = layer.pitch[index];
            let rows = if index == 0 {
                height
            } else {
                height.div_ceil(2)
            };
            let Some(minimum_pitch) = width.checked_mul(2) else {
                close_exported_fds(&descriptor);
                return Err("VVC P010 minimum pitch overflowed".to_owned());
            };
            let Some(end) = u64::from(pitch)
                .checked_mul(u64::from(rows))
                .and_then(|extent| u64::from(layer.offset[index]).checked_add(extent))
            else {
                close_exported_fds(&descriptor);
                return Err("VVC exported plane range overflowed".to_owned());
            };
            if pitch < minimum_pitch || end > u64::from(descriptor.objects[object_index].size) {
                close_exported_fds(&descriptor);
                return Err("VA exported a plane outside its DMA-BUF object".to_owned());
            }
            planes.push(VvcPlaneLayout {
                object_index,
                offset: layer.offset[index],
                pitch,
            });
        }
        let modifier = descriptor.objects[0].drm_format_modifier;
        let mut fds = BTreeSet::new();
        if descriptor.objects[..object_count].iter().any(|object| {
            object.drm_format_modifier != modifier || object.fd < 0 || !fds.insert(object.fd)
        }) {
            close_exported_fds(&descriptor);
            return Err("VA exported inconsistent VVC DMA-BUF objects".to_owned());
        }
        let dma_handles = descriptor.objects[..object_count]
            .iter()
            // SAFETY: Each validated nonnegative descriptor transfers one unique owned fd to File.
            .map(|object| unsafe { File::from_raw_fd(object.fd) })
            .collect();
        Ok(VvcVaapiFrame {
            dma_handles,
            modifier,
            planes,
        })
    }
}

impl Drop for VvcVaapiDecoder {
    fn drop(&mut self) {
        // SAFETY: These identifiers were created by this display and are released in dependency order.
        unsafe {
            if self.context != ffi::VA_INVALID_ID {
                ffi::vaDestroyContext(self.display, self.context);
            }
            ffi::vaDestroyConfig(self.display, self.config);
            ffi::vaTerminate(self.display);
        }
    }
}

fn validate_supported_picture(picture: &ParsedVvcPicture) -> Result<(), String> {
    if !picture.is_intra() {
        return Err("VVC VA-API decoder does not yet support inter pictures".to_owned());
    }
    if picture.slices.len() != 1 {
        return Err("VVC VA-API decoder currently requires one slice per picture".to_owned());
    }
    if !picture.pps.pps_no_pic_partition_flag || picture.pps.partition.is_some() {
        return Err("VVC VA-API decoder currently requires a single untiled picture".to_owned());
    }
    if picture.sps.sps_subpic_info_present_flag || picture.pps.pps_subpic_id_mapping_present_flag {
        return Err("VVC VA-API decoder does not yet support subpictures".to_owned());
    }
    let slice = &picture.slices[0];
    if picture.ph.ph_explicit_scaling_list_enabled_flag
        || slice.header.sh_explicit_scaling_list_used_flag
    {
        return Err("VVC VA-API decoder does not yet support scaling-list APS use".to_owned());
    }
    if !slice.header.sh_entry_point_offsets.is_empty() {
        return Err("VVC VA-API decoder does not yet support slice entry points".to_owned());
    }
    if picture.ph.ph_partition_constraints_override_flag {
        return Err(
            "VVC VA-API decoder does not yet support picture partition overrides".to_owned(),
        );
    }
    for id in required_alf_ids(picture) {
        if !picture.aps.iter().any(|aps| {
            aps.aps_params_type == ApsParamsType::Alf
                && aps.aps_adaptation_parameter_set_id == id
                && aps.alf_data.is_some()
        }) {
            let available = picture
                .aps
                .iter()
                .map(|aps| aps.aps_adaptation_parameter_set_id.to_string())
                .collect::<Vec<_>>()
                .join(",");
            return Err(format!(
                "VVC picture references unavailable ALF APS {id}; available ALF APS ids: {available}"
            ));
        }
    }
    if slice.header.sh_lmcs_used_flag
        && !picture.aps.iter().any(|aps| {
            aps.aps_params_type == ApsParamsType::Lmcs
                && aps.aps_adaptation_parameter_set_id == picture.ph.ph_lmcs_aps_id
                && aps.lmcs_data.is_some()
        })
    {
        return Err(format!(
            "VVC picture references unavailable LMCS APS {}",
            picture.ph.ph_lmcs_aps_id
        ));
    }
    Ok(())
}

fn required_alf_ids(picture: &ParsedVvcPicture) -> Vec<u8> {
    let slice = &picture.slices[0].header;
    let mut ids = Vec::new();
    if slice.sh_alf_enabled_flag {
        ids.extend(slice.sh_alf_aps_id_luma.iter().copied());
        if slice.sh_alf_cb_enabled_flag || slice.sh_alf_cr_enabled_flag {
            ids.push(slice.sh_alf_aps_id_chroma);
        }
        if slice.sh_alf_cc_cb_enabled_flag {
            ids.push(slice.sh_alf_cc_cb_aps_id);
        }
        if slice.sh_alf_cc_cr_enabled_flag {
            ids.push(slice.sh_alf_cc_cr_aps_id);
        }
    }
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn map_alf_parameters(aps: &AdaptationParameterSet) -> Result<ffi::VAAlfDataVVC, String> {
    if aps.aps_params_type != ApsParamsType::Alf {
        return Err("non-ALF APS reached the VA ALF mapper".to_owned());
    }
    let alf = aps
        .alf_data
        .as_ref()
        .ok_or_else(|| "VVC ALF APS has no parsed payload".to_owned())?;
    let mut value = ffi::VAAlfDataVVC {
        aps_adaptation_parameter_set_id: aps.aps_adaptation_parameter_set_id,
        ..Default::default()
    };
    if alf.alf_luma_filter_signal_flag {
        if alf.luma_coeff.len() != value.filtCoeff.len()
            || alf.luma_clip_idx.len() != value.alf_luma_clip_idx.len()
        {
            return Err("VVC ALF luma filter table is incomplete".to_owned());
        }
        value.alf_luma_num_filters_signalled_minus1 = u8::try_from(value.filtCoeff.len() - 1)
            .map_err(|_| "VVC ALF luma filter count exceeds the VA domain".to_owned())?;
        for index in 0..value.filtCoeff.len() {
            value.alf_luma_coeff_delta_idx[index] = u8::try_from(index)
                .map_err(|_| "VVC ALF luma filter index exceeds the VA domain".to_owned())?;
            for (destination, &source) in value.filtCoeff[index]
                .iter_mut()
                .zip(&alf.luma_coeff[index])
            {
                *destination = signed_narrow(source, "VVC ALF luma coefficient")?;
            }
            value.alf_luma_clip_idx[index].copy_from_slice(&alf.luma_clip_idx[index]);
        }
    }
    if alf.alf_chroma_filter_signal_flag {
        if alf.chroma_coeff.is_empty()
            || alf.chroma_coeff.len() > value.AlfCoeffC.len()
            || alf.chroma_clip_idx.len() != alf.chroma_coeff.len()
        {
            return Err("VVC ALF chroma filter table is inconsistent".to_owned());
        }
        value.alf_chroma_num_alt_filters_minus1 = u8::try_from(alf.chroma_coeff.len() - 1)
            .map_err(|_| "VVC ALF chroma filter count exceeds the VA domain".to_owned())?;
        for (index, coefficients) in alf.chroma_coeff.iter().enumerate() {
            for (destination, &source) in value.AlfCoeffC[index].iter_mut().zip(coefficients) {
                *destination = signed_narrow(source, "VVC ALF chroma coefficient")?;
            }
            value.alf_chroma_clip_idx[index].copy_from_slice(&alf.chroma_clip_idx[index]);
        }
    }
    map_cc_alf_coefficients(
        &alf.cc_cb_coeff,
        &mut value.CcAlfApsCoeffCb,
        &mut value.alf_cc_cb_filters_signalled_minus1,
        "Cb",
    )?;
    map_cc_alf_coefficients(
        &alf.cc_cr_coeff,
        &mut value.CcAlfApsCoeffCr,
        &mut value.alf_cc_cr_filters_signalled_minus1,
        "Cr",
    )?;
    let mut bits = ffi::_VAAlfDataVVC__bindgen_ty_1__bindgen_ty_1::default();
    bits.set_alf_luma_filter_signal_flag(alf.alf_luma_filter_signal_flag.into());
    bits.set_alf_chroma_filter_signal_flag(alf.alf_chroma_filter_signal_flag.into());
    bits.set_alf_cc_cb_filter_signal_flag(alf.alf_cc_cb_filter_signal_flag.into());
    bits.set_alf_cc_cr_filter_signal_flag(alf.alf_cc_cr_filter_signal_flag.into());
    bits.set_alf_luma_clip_flag(alf.alf_luma_clip_flag.into());
    bits.set_alf_chroma_clip_flag(alf.alf_chroma_clip_flag.into());
    value.alf_flags.bits = bits;
    Ok(value)
}

fn map_lmcs_parameters(aps: &AdaptationParameterSet) -> Result<ffi::VALmcsDataVVC, String> {
    if aps.aps_params_type != ApsParamsType::Lmcs {
        return Err("non-LMCS APS reached the VA LMCS mapper".to_owned());
    }
    let lmcs = aps
        .lmcs_data
        .as_ref()
        .ok_or_else(|| "VVC LMCS APS has no parsed payload".to_owned())?;
    let mut value = ffi::VALmcsDataVVC {
        aps_adaptation_parameter_set_id: aps.aps_adaptation_parameter_set_id,
        lmcs_min_bin_idx: lmcs.lmcs_min_bin_idx,
        lmcs_delta_max_bin_idx: lmcs.lmcs_delta_max_bin_idx,
        lmcsDeltaCrs: signed_narrow(lmcs.lmcs_delta_crs(), "VVC LMCS chroma residual delta")?,
        ..Default::default()
    };
    for index in 0..value.lmcsDeltaCW.len() {
        value.lmcsDeltaCW[index] =
            signed_narrow(lmcs.lmcs_delta_cw(index), "VVC LMCS codeword delta")?;
    }
    Ok(value)
}

fn map_cc_alf_coefficients(
    source: &[[i32; 7]],
    destination: &mut [[i8; 7]; 4],
    count_minus_one: &mut u8,
    label: &str,
) -> Result<(), String> {
    if source.is_empty() {
        return Ok(());
    }
    if source.len() > destination.len() {
        return Err(format!(
            "VVC {label} CC-ALF filter count exceeds the VA domain"
        ));
    }
    *count_minus_one = u8::try_from(source.len() - 1)
        .map_err(|_| format!("VVC {label} CC-ALF filter count exceeds the VA domain"))?;
    for (destination, source) in destination.iter_mut().zip(source) {
        for (destination, &source) in destination.iter_mut().zip(source) {
            *destination = signed_narrow(source, "VVC CC-ALF coefficient")?;
        }
    }
    Ok(())
}

fn map_picture_parameters(
    picture: &ParsedVvcPicture,
    surface: ffi::VASurfaceID,
) -> Result<ffi::VAPictureParameterBufferVVC, String> {
    let sps = picture.sps.as_ref();
    let pps = picture.pps.as_ref();
    let ph = picture.ph.as_ref();
    let mut value = ffi::VAPictureParameterBufferVVC {
        CurrPic: ffi::VAPictureVVC {
            picture_id: surface,
            pic_order_cnt: picture.poc,
            flags: 0,
            ..Default::default()
        },
        ..Default::default()
    };
    value.ReferenceFrames.fill(ffi::VAPictureVVC {
        picture_id: ffi::VA_INVALID_SURFACE,
        pic_order_cnt: 0,
        flags: ffi::VA_PICTURE_VVC_INVALID,
        ..Default::default()
    });
    value.pps_pic_width_in_luma_samples = narrow(pps.pps_pic_width_in_luma_samples, "PPS width")?;
    value.pps_pic_height_in_luma_samples =
        narrow(pps.pps_pic_height_in_luma_samples, "PPS height")?;
    value.sps_chroma_format_idc = sps.sps_chroma_format_idc;
    value.sps_bitdepth_minus8 = narrow(sps.sps_bitdepth_minus8, "SPS bit depth")?;
    value.sps_log2_ctu_size_minus5 = sps.sps_log2_ctu_size_minus5;
    value.sps_log2_min_luma_coding_block_size_minus2 = narrow(
        sps.partition_constraints
            .log2_min_luma_coding_block_size_minus2,
        "SPS minimum coding block size",
    )?;
    value.sps_log2_transform_skip_max_size_minus2 = narrow(
        sps.tool_flags.log2_transform_skip_max_size_minus2,
        "SPS transform skip size",
    )?;
    for (index, table) in sps.tool_flags.chroma_qp_tables.iter().take(3).enumerate() {
        for (destination, source) in value.ChromaQpTable[index].iter_mut().zip(table.build(12)) {
            *destination = i8::try_from(source)
                .map_err(|_| "SPS chroma QP table exceeds the VA domain".to_owned())?;
        }
    }
    value.sps_six_minus_max_num_merge_cand = narrow(
        sps.tool_flags.six_minus_max_num_merge_cand,
        "SPS merge candidates",
    )?;
    value.sps_five_minus_max_num_subblock_merge_cand = narrow(
        sps.tool_flags.five_minus_max_num_subblock_merge_cand,
        "SPS subblock merge candidates",
    )?;
    value.sps_max_num_merge_cand_minus_max_num_gpm_cand = narrow(
        sps.tool_flags.max_num_merge_cand_minus_max_num_gpm_cand,
        "SPS GPM merge candidates",
    )?;
    value.sps_log2_parallel_merge_level_minus2 = narrow(
        sps.tool_flags.log2_parallel_merge_level_minus2,
        "SPS parallel merge level",
    )?;
    value.sps_min_qp_prime_ts = narrow(sps.tool_flags.min_qp_prime_ts, "SPS transform QP")?;
    value.sps_six_minus_max_num_ibc_merge_cand = narrow(
        sps.tool_flags.six_minus_max_num_ibc_merge_cand,
        "SPS IBC merge candidates",
    )?;
    if sps.tool_flags.ladf_enabled_flag {
        let ladf = sps
            .tool_flags
            .ladf
            .as_ref()
            .ok_or_else(|| "VVC SPS enables LADF without parameters".to_owned())?;
        if ladf.intervals.len() > value.sps_ladf_qp_offset.len() {
            return Err("VVC SPS LADF interval count exceeds the VA domain".to_owned());
        }
        value.sps_num_ladf_intervals_minus2 = ladf.num_intervals_minus2;
        value.sps_ladf_lowest_interval_qp_offset = signed_narrow(
            ladf.lowest_interval_qp_offset,
            "SPS LADF lowest interval QP offset",
        )?;
        for (index, &(qp_offset, threshold)) in ladf.intervals.iter().enumerate() {
            value.sps_ladf_qp_offset[index] = signed_narrow(qp_offset, "SPS LADF QP offset")?;
            value.sps_ladf_delta_threshold_minus1[index] = narrow(threshold, "SPS LADF threshold")?;
        }
    }
    set_sps_flags(&mut value, sps);

    let virtual_boundaries = if sps.tool_flags.virtual_boundaries_present_flag {
        sps.tool_flags
            .virtual_boundaries
            .as_ref()
            .map(|boundaries| (&boundaries.pos_x_minus1, &boundaries.pos_y_minus1))
    } else if ph.ph_virtual_boundaries_present_flag {
        ph.ph_virtual_boundaries
            .as_ref()
            .map(|boundaries| (&boundaries.pos_x_minus1, &boundaries.pos_y_minus1))
    } else {
        None
    };
    if let Some((vertical, horizontal)) = virtual_boundaries {
        if vertical.len() > value.VirtualBoundaryPosX.len()
            || horizontal.len() > value.VirtualBoundaryPosY.len()
        {
            return Err("VVC virtual boundary count exceeds the VA domain".to_owned());
        }
        value.NumVerVirtualBoundaries = narrow(
            u32::try_from(vertical.len())
                .map_err(|_| "VVC vertical boundary count cannot be represented".to_owned())?,
            "VVC vertical boundary count",
        )?;
        value.NumHorVirtualBoundaries = narrow(
            u32::try_from(horizontal.len())
                .map_err(|_| "VVC horizontal boundary count cannot be represented".to_owned())?,
            "VVC horizontal boundary count",
        )?;
        for (destination, &minus_one) in value.VirtualBoundaryPosX.iter_mut().zip(vertical) {
            let position = minus_one
                .checked_add(1)
                .and_then(|value| value.checked_mul(8))
                .ok_or_else(|| "VVC vertical boundary position overflowed".to_owned())?;
            *destination = narrow(position, "VVC vertical boundary position")?;
        }
        for (destination, &minus_one) in value.VirtualBoundaryPosY.iter_mut().zip(horizontal) {
            let position = minus_one
                .checked_add(1)
                .and_then(|value| value.checked_mul(8))
                .ok_or_else(|| "VVC horizontal boundary position overflowed".to_owned())?;
            *destination = narrow(position, "VVC horizontal boundary position")?;
        }
    }

    if let Some(window) = pps.scaling_window {
        value.pps_scaling_win_left_offset = window.left_offset;
        value.pps_scaling_win_right_offset = window.right_offset;
        value.pps_scaling_win_top_offset = window.top_offset;
        value.pps_scaling_win_bottom_offset = window.bottom_offset;
    }
    value.pps_num_exp_tile_columns_minus1 = 0;
    value.pps_num_exp_tile_rows_minus1 = 0;
    value.pps_num_slices_in_pic_minus1 = 0;
    value.pps_pic_width_minus_wraparound_offset = narrow(
        pps.pps_pic_width_minus_wraparound_offset,
        "PPS wraparound offset",
    )?;
    value.pps_cb_qp_offset = signed_narrow(pps.pps_cb_qp_offset, "PPS Cb QP offset")?;
    value.pps_cr_qp_offset = signed_narrow(pps.pps_cr_qp_offset, "PPS Cr QP offset")?;
    value.pps_joint_cbcr_qp_offset_value = signed_narrow(
        pps.pps_joint_cbcr_qp_offset_value,
        "PPS joint chroma QP offset",
    )?;
    if pps.pps_cu_chroma_qp_offset_list_enabled_flag {
        let count = pps.pps_cb_qp_offset_list.len();
        if count == 0
            || count > value.pps_cb_qp_offset_list.len()
            || pps.pps_cr_qp_offset_list.len() != count
            || (!pps.pps_joint_cbcr_qp_offset_list.is_empty()
                && pps.pps_joint_cbcr_qp_offset_list.len() != count)
        {
            return Err("VVC PPS chroma QP offset list is inconsistent".to_owned());
        }
        value.pps_chroma_qp_offset_list_len_minus1 = narrow(
            u32::try_from(count - 1)
                .map_err(|_| "VVC chroma QP list length cannot be represented".to_owned())?,
            "VVC chroma QP list length",
        )?;
        for (destination, &source) in value
            .pps_cb_qp_offset_list
            .iter_mut()
            .zip(&pps.pps_cb_qp_offset_list)
        {
            *destination = signed_narrow(source, "PPS Cb QP offset list")?;
        }
        for (destination, &source) in value
            .pps_cr_qp_offset_list
            .iter_mut()
            .zip(&pps.pps_cr_qp_offset_list)
        {
            *destination = signed_narrow(source, "PPS Cr QP offset list")?;
        }
        for (destination, &source) in value
            .pps_joint_cbcr_qp_offset_list
            .iter_mut()
            .zip(&pps.pps_joint_cbcr_qp_offset_list)
        {
            *destination = signed_narrow(source, "PPS joint chroma QP offset list")?;
        }
    }
    set_pps_flags(&mut value, pps);

    value.ph_lmcs_aps_id = ph.ph_lmcs_aps_id;
    value.ph_scaling_list_aps_id = ph.ph_scaling_list_aps_id;
    let constraints = &sps.partition_constraints;
    value.ph_log2_diff_min_qt_min_cb_intra_slice_luma = narrow(
        constraints.log2_diff_min_qt_min_cb_intra_slice_luma,
        "PH intra luma minimum QT",
    )?;
    value.ph_max_mtt_hierarchy_depth_intra_slice_luma = narrow(
        constraints.max_mtt_hierarchy_depth_intra_slice_luma,
        "PH intra luma MTT depth",
    )?;
    value.ph_log2_diff_max_bt_min_qt_intra_slice_luma = narrow(
        constraints.log2_diff_max_bt_min_qt_intra_slice_luma,
        "PH intra luma maximum BT",
    )?;
    value.ph_log2_diff_max_tt_min_qt_intra_slice_luma = narrow(
        constraints.log2_diff_max_tt_min_qt_intra_slice_luma,
        "PH intra luma maximum TT",
    )?;
    value.ph_log2_diff_min_qt_min_cb_intra_slice_chroma = narrow(
        constraints.log2_diff_min_qt_min_cb_intra_slice_chroma,
        "PH intra chroma minimum QT",
    )?;
    value.ph_max_mtt_hierarchy_depth_intra_slice_chroma = narrow(
        constraints.max_mtt_hierarchy_depth_intra_slice_chroma,
        "PH intra chroma MTT depth",
    )?;
    value.ph_log2_diff_max_bt_min_qt_intra_slice_chroma = narrow(
        constraints.log2_diff_max_bt_min_qt_intra_slice_chroma,
        "PH intra chroma maximum BT",
    )?;
    value.ph_log2_diff_max_tt_min_qt_intra_slice_chroma = narrow(
        constraints.log2_diff_max_tt_min_qt_intra_slice_chroma,
        "PH intra chroma maximum TT",
    )?;
    value.ph_cu_qp_delta_subdiv_intra_slice = narrow(
        ph.ph_cu_qp_delta_subdiv_intra_slice,
        "PH intra QP subdivision",
    )?;
    value.ph_cu_chroma_qp_offset_subdiv_intra_slice = narrow(
        ph.ph_cu_chroma_qp_offset_subdiv_intra_slice,
        "PH intra chroma QP subdivision",
    )?;
    set_ph_flags(&mut value, ph);
    value.PicMiscFlags.value = u32::from(picture.is_intra());
    Ok(value)
}

fn map_slice_parameters(
    picture: &ParsedVvcPicture,
) -> Result<ffi::VASliceParameterBufferVVC, String> {
    let slice = &picture.slices[0];
    let header = &slice.header;
    let mut value = ffi::VASliceParameterBufferVVC {
        slice_data_size: u32::try_from(slice.nal.len())
            .map_err(|_| "VVC slice exceeds the VA data-size domain".to_owned())?,
        slice_data_offset: 0,
        slice_data_flag: ffi::VA_SLICE_DATA_FLAG_ALL,
        slice_data_byte_offset: slice.slice_data_byte_offset,
        ..Default::default()
    };
    value.RefPicList.fill([u8::MAX; 15]);
    value.sh_subpic_id = narrow(header.sh_subpic_id.unwrap_or(0), "slice subpicture id")?;
    value.sh_slice_address = narrow(header.sh_slice_address, "slice address")?;
    value.sh_num_tiles_in_slice_minus1 =
        narrow(header.sh_num_tiles_in_slice_minus1, "slice tile count")?;
    value.sh_slice_type = match header.sh_slice_type {
        SliceType::B => 0,
        SliceType::P => 1,
        SliceType::I => 2,
    };
    value.sh_num_alf_aps_ids_luma = header.sh_num_alf_aps_ids_luma;
    if header.sh_alf_aps_id_luma.len() > value.sh_alf_aps_id_luma.len() {
        return Err("VVC slice ALF luma APS count exceeds the VA domain".to_owned());
    }
    for (destination, &source) in value
        .sh_alf_aps_id_luma
        .iter_mut()
        .zip(&header.sh_alf_aps_id_luma)
    {
        *destination = source;
    }
    value.sh_alf_aps_id_chroma = header.sh_alf_aps_id_chroma;
    value.sh_alf_cc_cb_aps_id = header.sh_alf_cc_cb_aps_id;
    value.sh_alf_cc_cr_aps_id = header.sh_alf_cc_cr_aps_id;
    let qp_delta = if picture.pps.pps_qp_delta_info_in_ph_flag {
        picture.ph.ph_qp_delta
    } else {
        header.sh_qp_delta
    };
    value.SliceQpY = signed_narrow(
        26 + picture.pps.pps_init_qp_minus26 + qp_delta,
        "slice luma QP",
    )?;
    value.sh_cb_qp_offset = signed_narrow(header.sh_cb_qp_offset, "slice Cb QP offset")?;
    value.sh_cr_qp_offset = signed_narrow(header.sh_cr_qp_offset, "slice Cr QP offset")?;
    value.sh_joint_cbcr_qp_offset = signed_narrow(
        header.sh_joint_cbcr_qp_offset,
        "slice joint chroma QP offset",
    )?;
    value.sh_luma_beta_offset_div2 =
        signed_narrow(header.sh_luma_beta_offset_div2, "slice luma beta offset")?;
    value.sh_luma_tc_offset_div2 =
        signed_narrow(header.sh_luma_tc_offset_div2, "slice luma tc offset")?;
    value.sh_cb_beta_offset_div2 =
        signed_narrow(header.sh_cb_beta_offset_div2, "slice Cb beta offset")?;
    value.sh_cb_tc_offset_div2 = signed_narrow(header.sh_cb_tc_offset_div2, "slice Cb tc offset")?;
    value.sh_cr_beta_offset_div2 =
        signed_narrow(header.sh_cr_beta_offset_div2, "slice Cr beta offset")?;
    value.sh_cr_tc_offset_div2 = signed_narrow(header.sh_cr_tc_offset_div2, "slice Cr tc offset")?;
    set_slice_flags(&mut value, header);
    Ok(value)
}

fn set_sps_flags(
    value: &mut ffi::VAPictureParameterBufferVVC,
    sps: &oxideav_h266::sps::SeqParameterSet,
) {
    let flags = &sps.tool_flags;
    let mut bits = ffi::_VAPictureParameterBufferVVC__bindgen_ty_1__bindgen_ty_1::default();
    bits.set_sps_entropy_coding_sync_enabled_flag(sps.sps_entropy_coding_sync_enabled_flag.into());
    bits.set_sps_qtbtt_dual_tree_intra_flag(
        sps.partition_constraints.qtbtt_dual_tree_intra_flag.into(),
    );
    bits.set_sps_max_luma_transform_size_64_flag(
        sps.partition_constraints
            .max_luma_transform_size_64_flag
            .into(),
    );
    bits.set_sps_transform_skip_enabled_flag(flags.transform_skip_enabled_flag.into());
    bits.set_sps_bdpcm_enabled_flag(flags.bdpcm_enabled_flag.into());
    bits.set_sps_mts_enabled_flag(flags.mts_enabled_flag.into());
    bits.set_sps_explicit_mts_intra_enabled_flag(flags.explicit_mts_intra_enabled_flag.into());
    bits.set_sps_explicit_mts_inter_enabled_flag(flags.explicit_mts_inter_enabled_flag.into());
    bits.set_sps_lfnst_enabled_flag(flags.lfnst_enabled_flag.into());
    bits.set_sps_joint_cbcr_enabled_flag(flags.joint_cbcr_enabled_flag.into());
    bits.set_sps_same_qp_table_for_chroma_flag(flags.same_qp_table_for_chroma_flag.into());
    bits.set_sps_sao_enabled_flag(flags.sao_enabled_flag.into());
    bits.set_sps_alf_enabled_flag(flags.alf_enabled_flag.into());
    bits.set_sps_ccalf_enabled_flag(flags.ccalf_enabled_flag.into());
    bits.set_sps_lmcs_enabled_flag(flags.lmcs_enabled_flag.into());
    bits.set_sps_sbtmvp_enabled_flag(flags.sbtmvp_enabled_flag.into());
    bits.set_sps_amvr_enabled_flag(flags.amvr_enabled_flag.into());
    bits.set_sps_smvd_enabled_flag(flags.smvd_enabled_flag.into());
    bits.set_sps_mmvd_enabled_flag(flags.mmvd_enabled_flag.into());
    bits.set_sps_sbt_enabled_flag(flags.sbt_enabled_flag.into());
    bits.set_sps_affine_enabled_flag(flags.affine_enabled_flag.into());
    bits.set_sps_6param_affine_enabled_flag(flags.six_param_affine_enabled_flag.into());
    bits.set_sps_affine_amvr_enabled_flag(flags.affine_amvr_enabled_flag.into());
    bits.set_sps_affine_prof_enabled_flag(flags.affine_prof_enabled_flag.into());
    bits.set_sps_bcw_enabled_flag(flags.bcw_enabled_flag.into());
    bits.set_sps_ciip_enabled_flag(flags.ciip_enabled_flag.into());
    bits.set_sps_gpm_enabled_flag(flags.gpm_enabled_flag.into());
    bits.set_sps_isp_enabled_flag(flags.isp_enabled_flag.into());
    bits.set_sps_mrl_enabled_flag(flags.mrl_enabled_flag.into());
    bits.set_sps_mip_enabled_flag(flags.mip_enabled_flag.into());
    bits.set_sps_cclm_enabled_flag(flags.cclm_enabled_flag.into());
    bits.set_sps_chroma_horizontal_collocated_flag(flags.chroma_horizontal_collocated_flag.into());
    bits.set_sps_chroma_vertical_collocated_flag(flags.chroma_vertical_collocated_flag.into());
    bits.set_sps_palette_enabled_flag(flags.palette_enabled_flag.into());
    bits.set_sps_act_enabled_flag(flags.act_enabled_flag.into());
    bits.set_sps_ibc_enabled_flag(flags.ibc_enabled_flag.into());
    bits.set_sps_ladf_enabled_flag(flags.ladf_enabled_flag.into());
    bits.set_sps_explicit_scaling_list_enabled_flag(
        flags.explicit_scaling_list_enabled_flag.into(),
    );
    bits.set_sps_scaling_matrix_for_lfnst_disabled_flag(
        flags.scaling_matrix_for_lfnst_disabled_flag.into(),
    );
    bits.set_sps_scaling_matrix_for_alternative_colour_space_disabled_flag(
        flags
            .scaling_matrix_for_alternative_colour_space_disabled_flag
            .into(),
    );
    bits.set_sps_scaling_matrix_designated_colour_space_flag(
        flags.scaling_matrix_designated_colour_space_flag.into(),
    );
    bits.set_sps_virtual_boundaries_enabled_flag(flags.virtual_boundaries_enabled_flag.into());
    bits.set_sps_virtual_boundaries_present_flag(flags.virtual_boundaries_present_flag.into());
    value.sps_flags.bits = bits;
}

fn set_pps_flags(
    value: &mut ffi::VAPictureParameterBufferVVC,
    pps: &oxideav_h266::pps::PicParameterSet,
) {
    let mut bits = ffi::_VAPictureParameterBufferVVC__bindgen_ty_2__bindgen_ty_1::default();
    bits.set_pps_rect_slice_flag(pps.pps_rect_slice_flag.into());
    bits.set_pps_single_slice_per_subpic_flag(pps.pps_single_slice_per_subpic_flag.into());
    bits.set_pps_loop_filter_across_slices_enabled_flag(
        pps.pps_loop_filter_across_slices_enabled_flag.into(),
    );
    bits.set_pps_weighted_pred_flag(pps.pps_weighted_pred_flag.into());
    bits.set_pps_weighted_bipred_flag(pps.pps_weighted_bipred_flag.into());
    bits.set_pps_ref_wraparound_enabled_flag(pps.pps_ref_wraparound_enabled_flag.into());
    bits.set_pps_cu_qp_delta_enabled_flag(pps.pps_cu_qp_delta_enabled_flag.into());
    bits.set_pps_cu_chroma_qp_offset_list_enabled_flag(
        pps.pps_cu_chroma_qp_offset_list_enabled_flag.into(),
    );
    bits.set_pps_deblocking_filter_override_enabled_flag(
        pps.pps_deblocking_filter_override_enabled_flag.into(),
    );
    bits.set_pps_deblocking_filter_disabled_flag(pps.pps_deblocking_filter_disabled_flag.into());
    bits.set_pps_dbf_info_in_ph_flag(pps.pps_dbf_info_in_ph_flag.into());
    bits.set_pps_sao_info_in_ph_flag(pps.pps_sao_info_in_ph_flag.into());
    bits.set_pps_alf_info_in_ph_flag(pps.pps_alf_info_in_ph_flag.into());
    value.pps_flags.bits = bits;
}

fn set_ph_flags(
    value: &mut ffi::VAPictureParameterBufferVVC,
    ph: &oxideav_h266::picture_header::PictureHeader,
) {
    let mut bits = ffi::_VAPictureParameterBufferVVC__bindgen_ty_3__bindgen_ty_1::default();
    bits.set_ph_non_ref_pic_flag(ph.ph_non_ref_pic_flag.into());
    bits.set_ph_alf_enabled_flag(ph.ph_alf_enabled_flag.into());
    bits.set_ph_alf_cb_enabled_flag(ph.ph_alf_cb_enabled_flag.into());
    bits.set_ph_alf_cr_enabled_flag(ph.ph_alf_cr_enabled_flag.into());
    bits.set_ph_alf_cc_cb_enabled_flag(ph.ph_alf_cc_cb_enabled_flag.into());
    bits.set_ph_alf_cc_cr_enabled_flag(ph.ph_alf_cc_cr_enabled_flag.into());
    bits.set_ph_lmcs_enabled_flag(ph.ph_lmcs_enabled_flag.into());
    bits.set_ph_chroma_residual_scale_flag(ph.ph_chroma_residual_scale_flag.into());
    bits.set_ph_explicit_scaling_list_enabled_flag(ph.ph_explicit_scaling_list_enabled_flag.into());
    bits.set_ph_virtual_boundaries_present_flag(ph.ph_virtual_boundaries_present_flag.into());
    bits.set_ph_temporal_mvp_enabled_flag(ph.ph_temporal_mvp_enabled_flag.into());
    bits.set_ph_mmvd_fullpel_only_flag(ph.ph_mmvd_fullpel_only_flag.into());
    bits.set_ph_mvd_l1_zero_flag(ph.ph_mvd_l1_zero_flag.into());
    bits.set_ph_bdof_disabled_flag(ph.ph_bdof_disabled_flag.into());
    bits.set_ph_dmvr_disabled_flag(ph.ph_dmvr_disabled_flag.into());
    bits.set_ph_prof_disabled_flag(ph.ph_prof_disabled_flag.into());
    bits.set_ph_joint_cbcr_sign_flag(ph.ph_joint_cbcr_sign_flag.into());
    bits.set_ph_sao_luma_enabled_flag(ph.ph_sao_luma_enabled_flag.into());
    bits.set_ph_sao_chroma_enabled_flag(ph.ph_sao_chroma_enabled_flag.into());
    bits.set_ph_deblocking_filter_disabled_flag(ph.deblocking.filter_disabled_flag.into());
    value.ph_flags.bits = bits;
}

fn set_slice_flags(
    value: &mut ffi::VASliceParameterBufferVVC,
    header: &oxideav_h266::slice_header::StatefulSliceHeader,
) {
    let mut bits = ffi::_VASliceParameterBufferVVC__bindgen_ty_1__bindgen_ty_1::default();
    bits.set_sh_alf_enabled_flag(header.sh_alf_enabled_flag.into());
    bits.set_sh_alf_cb_enabled_flag(header.sh_alf_cb_enabled_flag.into());
    bits.set_sh_alf_cr_enabled_flag(header.sh_alf_cr_enabled_flag.into());
    bits.set_sh_alf_cc_cb_enabled_flag(header.sh_alf_cc_cb_enabled_flag.into());
    bits.set_sh_alf_cc_cr_enabled_flag(header.sh_alf_cc_cr_enabled_flag.into());
    bits.set_sh_lmcs_used_flag(header.sh_lmcs_used_flag.into());
    bits.set_sh_explicit_scaling_list_used_flag(header.sh_explicit_scaling_list_used_flag.into());
    bits.set_sh_cabac_init_flag(header.sh_cabac_init_flag.into());
    bits.set_sh_cu_chroma_qp_offset_enabled_flag(header.sh_cu_chroma_qp_offset_enabled_flag.into());
    bits.set_sh_sao_luma_used_flag(header.sh_sao_luma_used_flag.into());
    bits.set_sh_sao_chroma_used_flag(header.sh_sao_chroma_used_flag.into());
    bits.set_sh_deblocking_filter_disabled_flag(header.sh_deblocking_filter_disabled_flag.into());
    bits.set_sh_dep_quant_used_flag(header.sh_dep_quant_used_flag.into());
    bits.set_sh_sign_data_hiding_used_flag(header.sh_sign_data_hiding_used_flag.into());
    bits.set_sh_ts_residual_coding_disabled_flag(header.sh_ts_residual_coding_disabled_flag.into());
    value.sh_flags.bits = bits;
}

fn narrow<T>(value: u32, label: &str) -> Result<T, String>
where
    T: TryFrom<u32>,
{
    T::try_from(value).map_err(|_| format!("{label} exceeds the VA domain"))
}

fn signed_narrow<T>(value: i32, label: &str) -> Result<T, String>
where
    T: TryFrom<i32>,
{
    T::try_from(value).map_err(|_| format!("{label} exceeds the VA domain"))
}

fn va_status(status: ffi::VAStatus) -> Result<(), String> {
    if status == ffi::VA_STATUS_SUCCESS as ffi::VAStatus {
        return Ok(());
    }
    // SAFETY: libva accepts every VAStatus value and returns null or a static C string.
    let message = unsafe { ffi::vaErrorStr(status) };
    if message.is_null() {
        Err(format!("VA status {status}"))
    } else {
        // SAFETY: The nonnull pointer returned by libva references a static NUL-terminated string.
        Err(unsafe { CStr::from_ptr(message) }
            .to_string_lossy()
            .into_owned())
    }
}

fn close_exported_fds(descriptor: &ffi::VADRMPRIMESurfaceDescriptor) {
    let count = usize::try_from(descriptor.num_objects)
        .unwrap_or(0)
        .min(MAX_EXPORTED_OBJECTS);
    let mut closed = BTreeSet::new();
    for object in &descriptor.objects[..count] {
        if object.fd >= 0 && closed.insert(object.fd) {
            // SAFETY: Rejected exported descriptors still transfer ownership of each unique fd.
            unsafe {
                drop(File::from_raw_fd(object.fd));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxideav_h266::nal::iter_annex_b;

    #[test]
    fn va_domain_narrowing_is_checked() {
        assert_eq!(narrow::<u8>(255, "test").unwrap(), 255);
        assert!(narrow::<u8>(256, "test").is_err());
        assert_eq!(signed_narrow::<i8>(-128, "test").unwrap(), -128);
        assert!(signed_narrow::<i8>(128, "test").is_err());
    }

    #[test]
    fn real_fixture_maps_into_checked_va_parameters_when_requested() {
        let Some(path) = std::env::var_os("SUPERI_VVC_FIXTURE") else {
            return;
        };
        let data = std::fs::read(path).unwrap();
        let mut access_unit = Vec::new();
        for unit in iter_annex_b(&data) {
            access_unit.extend_from_slice(&[0, 0, 0, 1]);
            access_unit.extend_from_slice(unit.raw);
            if unit.header.nal_unit_type.is_vcl() {
                break;
            }
        }
        let picture = VvcBitstreamParser::default()
            .parse_access_unit(17, &access_unit)
            .unwrap()
            .unwrap();
        validate_supported_picture(&picture).unwrap();
        let parameters = map_picture_parameters(&picture, 23).unwrap();
        let slice = map_slice_parameters(&picture).unwrap();
        for aps in &picture.aps {
            match aps.aps_params_type {
                ApsParamsType::Alf => {
                    map_alf_parameters(aps).unwrap();
                }
                ApsParamsType::Lmcs => {
                    map_lmcs_parameters(aps).unwrap();
                }
                ApsParamsType::Scaling | ApsParamsType::Reserved(_) => {}
            }
        }

        assert_eq!(parameters.CurrPic.picture_id, 23);
        assert_eq!(parameters.pps_pic_width_in_luma_samples, 416);
        assert_eq!(parameters.pps_pic_height_in_luma_samples, 240);
        assert_eq!(slice.slice_data_size as usize, picture.slices[0].nal.len());
        assert_eq!(slice.slice_data_byte_offset, 9);
        assert!(slice.slice_data_byte_offset < slice.slice_data_size);
    }
}

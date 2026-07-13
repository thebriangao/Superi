use superi_color::hdr::{
    convert_relative_transfer, decode_relative_transfer, encode_relative_transfer, hlg_eotf,
    hlg_inverse_eotf, hlg_inverse_oetf, hlg_oetf, pq_eotf, pq_inverse_eotf, EncodedSignal,
    HlgDisplayParameters, Nits, NormalizedSignal, RelativeLight,
};
use superi_core::color_space::TransferFunction;
use superi_core::error::{ErrorCategory, Recoverability};

const STRICT: f64 = 2.0e-12;

#[test]
fn sdr_curves_match_reference_anchors_and_keep_extended_values() {
    assert_close(
        decode(TransferFunction::Srgb, 0.04045),
        0.003_130_804_953_560_371_3,
        STRICT,
    );
    assert_close(
        encode(TransferFunction::Srgb, 0.003_130_8),
        0.040_449_936,
        STRICT,
    );
    assert_close(
        encode(TransferFunction::Bt709, 0.18),
        0.409_007_728_864_150_4,
        STRICT,
    );
    assert_close(
        encode(TransferFunction::Bt2020TenBit, 0.18),
        0.408_848_108_891_225,
        STRICT,
    );
    assert_close(
        encode(TransferFunction::Bt2020TwelveBit, 0.18),
        0.408_846_402_493_503_7,
        STRICT,
    );
    assert_close(
        encode(TransferFunction::Gamma22, 0.18),
        0.458_656_446_864_381_1,
        STRICT,
    );
    assert_close(
        encode(TransferFunction::Gamma24, 0.18),
        0.489_437_089_573_878_3,
        STRICT,
    );

    for transfer in [
        TransferFunction::Linear,
        TransferFunction::Srgb,
        TransferFunction::Bt709,
        TransferFunction::Bt2020TenBit,
        TransferFunction::Bt2020TwelveBit,
        TransferFunction::Gamma22,
        TransferFunction::Gamma24,
    ] {
        for linear in [-0.25, -0.001, 0.0, 0.18, 1.0, 1.5] {
            let encoded = encode_relative_transfer(transfer, relative(linear)).unwrap();
            let decoded = decode_relative_transfer(transfer, encoded).unwrap().value();
            assert_close(decoded, linear, 8.0e-12);
        }
    }
}

#[test]
fn relative_conversion_decodes_before_it_encodes() {
    let source = 0.735_356_983_052_449_5;
    let expected_linear = 0.5;
    let converted = convert_relative_transfer(
        TransferFunction::Srgb,
        TransferFunction::Bt709,
        signal(source),
    )
    .unwrap();

    assert_close(
        decode(TransferFunction::Srgb, source),
        expected_linear,
        STRICT,
    );
    assert_close(
        decode_relative_transfer(TransferFunction::Bt709, converted)
            .unwrap()
            .value(),
        expected_linear,
        STRICT,
    );
    assert_close(
        convert_relative_transfer(TransferFunction::Bt709, TransferFunction::Srgb, converted)
            .unwrap()
            .value(),
        source,
        4.0e-12,
    );
}

#[test]
fn pq_uses_absolute_luminance_and_preserves_precision() {
    assert_close(pq_eotf(normalized(0.0)).unwrap().value(), 0.0, STRICT);
    assert_close(pq_eotf(normalized(1.0)).unwrap().value(), 10_000.0, 2.0e-8);
    assert_close(
        pq_eotf(normalized(0.508_078_421_517_399)).unwrap().value(),
        100.0,
        2.0e-9,
    );

    for luminance_nits in [0.0, 0.0001, 0.1, 100.0, 203.0, 1_000.0, 10_000.0] {
        let signal = pq_inverse_eotf(nits(luminance_nits)).unwrap();
        assert_close(pq_eotf(signal).unwrap().value(), luminance_nits, 3.0e-8);
    }
}

#[test]
fn hlg_scene_curve_and_display_rendering_keep_distinct_semantics() {
    assert_close(hlg_oetf(relative(0.0)).unwrap().value(), 0.0, STRICT);
    assert_close(hlg_oetf(relative(1.0 / 12.0)).unwrap().value(), 0.5, STRICT);
    assert_close(hlg_oetf(relative(1.0)).unwrap().value(), 1.0, 4.0e-8);

    for scene_linear in [0.0, 0.0001, 1.0 / 12.0, 0.18, 1.0, 2.0] {
        let signal = hlg_oetf(relative(scene_linear)).unwrap();
        assert_close(
            hlg_inverse_oetf(signal).unwrap().value(),
            scene_linear,
            2.0e-8,
        );
        assert_close(
            decode_relative_transfer(TransferFunction::Hlg, signal)
                .unwrap()
                .value(),
            scene_linear,
            2.0e-8,
        );
    }

    let display = HlgDisplayParameters::new(nits(1_000.0), nits(0.0)).unwrap();
    assert_close(display.nominal_peak_luminance().value(), 1_000.0, STRICT);
    assert_close(display.black_luminance().value(), 0.0, STRICT);
    assert_close(display.system_gamma(), 1.2, STRICT);
    assert_rgb_close(
        nits_values(hlg_eotf(signals([1.0, 1.0, 1.0]), display).unwrap()),
        [1_000.0, 1_000.0, 1_000.0],
        5.0e-5,
    );
    let middle = 1_000.0 * (1.0_f64 / 12.0).powf(1.2);
    assert_rgb_close(
        nits_values(hlg_eotf(signals([0.5, 0.5, 0.5]), display).unwrap()),
        [middle; 3],
        2.0e-8,
    );

    let encoded_color = [0.75, 0.5, 0.25];
    let scene_color =
        encoded_color.map(|component| hlg_inverse_oetf(signal(component)).unwrap().value());
    let display_color = nits_values(hlg_eotf(signals(encoded_color), display).unwrap());
    assert_close(
        display_color[0] / display_color[1],
        scene_color[0] / scene_color[1],
        STRICT,
    );

    let unity_gamma =
        HlgDisplayParameters::with_system_gamma(nits(1_000.0), nits(0.0), 1.0).unwrap();
    assert_rgb_close(
        nits_values(hlg_eotf(signals([0.5; 3]), unity_gamma).unwrap()),
        [1_000.0 / 12.0; 3],
        2.0e-10,
    );
}

#[test]
fn hlg_display_round_trips_color_with_black_lift_and_headroom() {
    let display = HlgDisplayParameters::new(nits(600.0), nits(0.005)).unwrap();
    let black = hlg_eotf(signals([0.0; 3]), display).unwrap();
    assert_rgb_close(nits_values(black), [0.005; 3], 2.0e-11);

    for signal in [
        [0.0, 0.0, 0.0],
        [0.18, 0.25, 0.4],
        [0.75, 0.5, 0.25],
        [1.1, 1.05, 1.2],
    ] {
        let display_light = hlg_eotf(signals(signal), display).unwrap();
        let restored = hlg_inverse_eotf(display_light, display).unwrap();
        assert_rgb_close(signal_values(restored), signal, 5.0e-10);
    }
}

#[test]
fn transfer_domains_fail_explicitly_instead_of_clamping_or_guessing() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_invalid(RelativeLight::new(value));
        assert_invalid(EncodedSignal::new(value));
        assert_invalid(NormalizedSignal::new(value));
        assert_invalid(Nits::new(value));
    }

    assert_invalid(Nits::new(-0.001));
    assert_invalid(encode_relative_transfer(
        TransferFunction::Unspecified,
        relative(0.5),
    ));
    assert_invalid(decode_relative_transfer(
        TransferFunction::Unspecified,
        signal(0.5),
    ));
    assert_invalid(encode_relative_transfer(
        TransferFunction::Pq,
        relative(100.0),
    ));
    assert_invalid(decode_relative_transfer(TransferFunction::Pq, signal(0.5)));
    assert_invalid(NormalizedSignal::new(-0.001));
    assert_invalid(NormalizedSignal::new(1.001));
    assert_invalid(pq_inverse_eotf(nits(10_000.001)));
    assert_invalid(hlg_oetf(relative(-0.001)));
    assert_invalid(HlgDisplayParameters::new(nits(0.0), nits(0.0)));
    assert_invalid(HlgDisplayParameters::new(nits(1_000.0), nits(1_000.0)));
}

#[test]
fn large_finite_inputs_never_produce_successful_nonfinite_results() {
    assert_invalid(decode_relative_transfer(
        TransferFunction::Srgb,
        signal(f64::MAX),
    ));
    assert_invalid(decode_relative_transfer(
        TransferFunction::Gamma24,
        signal(f64::MAX),
    ));
    assert_invalid(hlg_oetf(relative(f64::MAX)));
    assert_invalid(hlg_inverse_oetf(signal(128.0)));

    let display = HlgDisplayParameters::new(nits(1_000.0), nits(0.0)).unwrap();
    assert_invalid(hlg_eotf(signals([128.0; 3]), display));
}

#[test]
fn transfer_contracts_are_send_sync_and_copyable() {
    fn assert_send_sync_copy<T: Send + Sync + Copy>() {}
    assert_send_sync_copy::<RelativeLight>();
    assert_send_sync_copy::<EncodedSignal>();
    assert_send_sync_copy::<NormalizedSignal>();
    assert_send_sync_copy::<Nits>();
    assert_send_sync_copy::<HlgDisplayParameters>();

    let _: fn(TransferFunction, EncodedSignal) -> superi_core::error::Result<RelativeLight> =
        decode_relative_transfer;
    let _: fn(TransferFunction, RelativeLight) -> superi_core::error::Result<EncodedSignal> =
        encode_relative_transfer;
    let _: fn(NormalizedSignal) -> superi_core::error::Result<Nits> = pq_eotf;
    let _: fn(Nits) -> superi_core::error::Result<NormalizedSignal> = pq_inverse_eotf;
}

fn assert_invalid<T>(result: superi_core::error::Result<T>) {
    let error = result.err().expect("operation should reject invalid input");
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert!(!error.contexts().is_empty());
}

fn assert_close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected:.17}, got {actual:.17}, tolerance {tolerance:.3e}"
    );
}

fn assert_rgb_close(actual: [f64; 3], expected: [f64; 3], tolerance: f64) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert_close(actual, expected, tolerance);
    }
}

fn relative(value: f64) -> RelativeLight {
    RelativeLight::new(value).unwrap()
}

fn signal(value: f64) -> EncodedSignal {
    EncodedSignal::new(value).unwrap()
}

fn normalized(value: f64) -> NormalizedSignal {
    NormalizedSignal::new(value).unwrap()
}

fn nits(value: f64) -> Nits {
    Nits::new(value).unwrap()
}

fn signals(values: [f64; 3]) -> [EncodedSignal; 3] {
    values.map(signal)
}

fn signal_values(values: [EncodedSignal; 3]) -> [f64; 3] {
    values.map(EncodedSignal::value)
}

fn nits_values(values: [Nits; 3]) -> [f64; 3] {
    values.map(Nits::value)
}

fn encode(transfer: TransferFunction, value: f64) -> f64 {
    encode_relative_transfer(transfer, relative(value))
        .unwrap()
        .value()
}

fn decode(transfer: TransferFunction, value: f64) -> f64 {
    decode_relative_transfer(transfer, signal(value))
        .unwrap()
        .value()
}

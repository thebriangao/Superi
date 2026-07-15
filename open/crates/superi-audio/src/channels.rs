//! Common channel layouts and explicit real-time-safe channel conversion.
//!
//! Speaker conversion follows the mono, stereo, quad, and 5.1 mixing rules standardized by the
//! Web Audio speaker interpretation. Discrete conversion copies channels by stream index. The
//! conversion matrix is prepared before processing so the callback path allocates and locks
//! nothing.

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::ChannelLayout;

use crate::graph::{AudioProcessBlock, AudioProcessor};

const COMPONENT: &str = "superi-audio.channels";
const ROOT_HALF: f32 = 0.707_106_77;

/// A canonical channel layout supported throughout the audio engine.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum CommonChannelLayout {
    /// One front-center channel.
    Mono,
    /// Front-left and front-right channels.
    Stereo,
    /// Front-left, front-right, back-left, and back-right channels.
    Quad,
    /// Front-left, front-right, front-center, LFE, back-left, and back-right channels.
    Surround5_1,
    /// The 5.1 order followed by side-left and side-right channels.
    Surround7_1,
}

impl CommonChannelLayout {
    /// Returns the canonical ordered core layout.
    #[must_use]
    pub fn layout(self) -> ChannelLayout {
        match self {
            Self::Mono => ChannelLayout::mono(),
            Self::Stereo => ChannelLayout::stereo(),
            Self::Quad => ChannelLayout::quad(),
            Self::Surround5_1 => ChannelLayout::surround_5_1(),
            Self::Surround7_1 => ChannelLayout::surround_7_1(),
        }
    }

    /// Classifies an exact canonical ordered layout.
    #[must_use]
    pub fn from_layout(layout: &ChannelLayout) -> Option<Self> {
        [
            Self::Mono,
            Self::Stereo,
            Self::Quad,
            Self::Surround5_1,
            Self::Surround7_1,
        ]
        .into_iter()
        .find(|candidate| candidate.layout() == *layout)
    }
}

/// The caller-selected meaning of channel conversion.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ChannelInterpretation {
    /// Convert semantic speaker layouts with standardized coefficients.
    Speakers,
    /// Copy channels by stream index, drop excess inputs, and zero-fill excess outputs.
    Discrete,
}

/// A precomputed channel conversion implementing the audio graph processor contract.
pub struct PreparedChannelMixer {
    input_layout: ChannelLayout,
    output_layout: ChannelLayout,
    matrix: Box<[f32]>,
}

impl PreparedChannelMixer {
    /// Prepares one immutable conversion matrix outside the real-time callback.
    pub fn new(
        input_layout: ChannelLayout,
        output_layout: ChannelLayout,
        interpretation: ChannelInterpretation,
    ) -> Result<Self> {
        let input_channels = input_layout.len();
        let output_channels = output_layout.len();
        let coefficient_count = input_channels.checked_mul(output_channels).ok_or_else(|| {
            channel_error(
                "prepare",
                "channel conversion matrix dimensions overflow",
                input_channels,
                output_channels,
            )
        })?;
        let mut matrix = vec![0.0; coefficient_count];

        match interpretation {
            ChannelInterpretation::Discrete => {
                for channel in 0..input_channels.min(output_channels) {
                    matrix[channel * input_channels + channel] = 1.0;
                }
            }
            ChannelInterpretation::Speakers => {
                prepare_speaker_matrix(&input_layout, &output_layout, input_channels, &mut matrix)?
            }
        }

        Ok(Self {
            input_layout,
            output_layout,
            matrix: matrix.into_boxed_slice(),
        })
    }

    /// Returns the exact required input layout.
    #[must_use]
    pub const fn input_layout(&self) -> &ChannelLayout {
        &self.input_layout
    }

    /// Returns the exact produced output layout.
    #[must_use]
    pub const fn output_layout(&self) -> &ChannelLayout {
        &self.output_layout
    }
}

impl AudioProcessor for PreparedChannelMixer {
    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
        let input_channels = self.input_layout.len();
        let output_channels = self.output_layout.len();
        let input = block.input.ok_or_else(|| {
            channel_error(
                "process",
                "channel conversion requires one input",
                input_channels,
                output_channels,
            )
        })?;
        if block.input_layout != Some(&self.input_layout)
            || block.output_layout != &self.output_layout
        {
            return Err(channel_error(
                "process",
                "process block layouts do not match the prepared conversion",
                input_channels,
                output_channels,
            ));
        }
        let expected_input = block
            .frame_count
            .checked_mul(input_channels)
            .ok_or_else(|| {
                channel_error(
                    "process",
                    "input sample count overflow",
                    input_channels,
                    output_channels,
                )
            })?;
        let expected_output = block
            .frame_count
            .checked_mul(output_channels)
            .ok_or_else(|| {
                channel_error(
                    "process",
                    "output sample count overflow",
                    input_channels,
                    output_channels,
                )
            })?;
        if input.len() != expected_input || block.output.len() != expected_output {
            return Err(channel_error(
                "process",
                "process block sample count does not match its layouts and frame count",
                input_channels,
                output_channels,
            ));
        }
        if input.iter().any(|sample| !sample.is_finite()) {
            return Err(channel_error(
                "process",
                "channel conversion input contains a non-finite sample",
                input_channels,
                output_channels,
            ));
        }

        for frame in 0..block.frame_count {
            let input_frame = &input[frame * input_channels..(frame + 1) * input_channels];
            let output_frame =
                &mut block.output[frame * output_channels..(frame + 1) * output_channels];
            for (output_channel, sample) in output_frame.iter_mut().enumerate() {
                let coefficients = &self.matrix
                    [output_channel * input_channels..(output_channel + 1) * input_channels];
                *sample = coefficients
                    .iter()
                    .zip(input_frame)
                    .map(|(coefficient, input)| coefficient * input)
                    .sum();
            }
        }
        Ok(())
    }
}

fn prepare_speaker_matrix(
    input_layout: &ChannelLayout,
    output_layout: &ChannelLayout,
    input_channels: usize,
    matrix: &mut [f32],
) -> Result<()> {
    if input_layout == output_layout {
        for channel in 0..input_channels {
            matrix[channel * input_channels + channel] = 1.0;
        }
        return Ok(());
    }

    let input = CommonChannelLayout::from_layout(input_layout);
    let output = CommonChannelLayout::from_layout(output_layout);
    match (input, output) {
        (Some(CommonChannelLayout::Mono), Some(CommonChannelLayout::Stereo)) => {
            set_rows(matrix, input_channels, &[&[1.0], &[1.0]]);
        }
        (Some(CommonChannelLayout::Mono), Some(CommonChannelLayout::Quad)) => {
            set_rows(matrix, input_channels, &[&[1.0], &[1.0], &[0.0], &[0.0]]);
        }
        (Some(CommonChannelLayout::Mono), Some(CommonChannelLayout::Surround5_1)) => {
            set_rows(
                matrix,
                input_channels,
                &[&[0.0], &[0.0], &[1.0], &[0.0], &[0.0], &[0.0]],
            );
        }
        (Some(CommonChannelLayout::Stereo), Some(CommonChannelLayout::Mono)) => {
            set_rows(matrix, input_channels, &[&[0.5, 0.5]]);
        }
        (Some(CommonChannelLayout::Stereo), Some(CommonChannelLayout::Quad)) => {
            set_rows(
                matrix,
                input_channels,
                &[&[1.0, 0.0], &[0.0, 1.0], &[0.0, 0.0], &[0.0, 0.0]],
            );
        }
        (Some(CommonChannelLayout::Stereo), Some(CommonChannelLayout::Surround5_1)) => {
            set_rows(
                matrix,
                input_channels,
                &[
                    &[1.0, 0.0],
                    &[0.0, 1.0],
                    &[0.0, 0.0],
                    &[0.0, 0.0],
                    &[0.0, 0.0],
                    &[0.0, 0.0],
                ],
            );
        }
        (Some(CommonChannelLayout::Quad), Some(CommonChannelLayout::Mono)) => {
            set_rows(matrix, input_channels, &[&[0.25, 0.25, 0.25, 0.25]]);
        }
        (Some(CommonChannelLayout::Quad), Some(CommonChannelLayout::Stereo)) => {
            set_rows(
                matrix,
                input_channels,
                &[&[0.5, 0.0, 0.5, 0.0], &[0.0, 0.5, 0.0, 0.5]],
            );
        }
        (Some(CommonChannelLayout::Quad), Some(CommonChannelLayout::Surround5_1)) => {
            set_rows(
                matrix,
                input_channels,
                &[
                    &[1.0, 0.0, 0.0, 0.0],
                    &[0.0, 1.0, 0.0, 0.0],
                    &[0.0, 0.0, 0.0, 0.0],
                    &[0.0, 0.0, 0.0, 0.0],
                    &[0.0, 0.0, 1.0, 0.0],
                    &[0.0, 0.0, 0.0, 1.0],
                ],
            );
        }
        (Some(CommonChannelLayout::Surround5_1), Some(CommonChannelLayout::Mono)) => {
            set_rows(
                matrix,
                input_channels,
                &[&[ROOT_HALF, ROOT_HALF, 1.0, 0.0, 0.5, 0.5]],
            );
        }
        (Some(CommonChannelLayout::Surround5_1), Some(CommonChannelLayout::Stereo)) => {
            set_rows(
                matrix,
                input_channels,
                &[
                    &[1.0, 0.0, ROOT_HALF, 0.0, ROOT_HALF, 0.0],
                    &[0.0, 1.0, ROOT_HALF, 0.0, 0.0, ROOT_HALF],
                ],
            );
        }
        (Some(CommonChannelLayout::Surround5_1), Some(CommonChannelLayout::Quad)) => {
            set_rows(
                matrix,
                input_channels,
                &[
                    &[1.0, 0.0, ROOT_HALF, 0.0, 0.0, 0.0],
                    &[0.0, 1.0, ROOT_HALF, 0.0, 0.0, 0.0],
                    &[0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
                    &[0.0, 0.0, 0.0, 0.0, 0.0, 1.0],
                ],
            );
        }
        _ => {
            return Err(channel_error(
                "prepare",
                "speaker conversion is not defined for these channel layouts",
                input_layout.len(),
                output_layout.len(),
            ));
        }
    }
    Ok(())
}

fn set_rows(matrix: &mut [f32], input_channels: usize, rows: &[&[f32]]) {
    for (output_channel, row) in rows.iter().enumerate() {
        matrix[output_channel * input_channels..(output_channel + 1) * input_channels]
            .copy_from_slice(row);
    }
}

fn channel_error(
    operation: &'static str,
    message: &'static str,
    input_channels: usize,
    output_channels: usize,
) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("input_channels", input_channels.to_string())
            .with_field("output_channels", output_channels.to_string()),
    )
}

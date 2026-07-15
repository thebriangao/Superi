//! Deterministic unity routing for submix, auxiliary, and master buses.
//!
//! [`SummingBus`] combines exact-layout inputs in the stable route order prepared by
//! [`crate::graph::AudioGraph`]. Gain, fades, pan, mute, solo, phase, channel mapping, effects,
//! and automation remain separate processing concerns.

use crate::graph::{AudioProcessBlock, AudioProcessor};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

const COMPONENT: &str = "superi-audio.routing";

/// Allocation-free unity summing for a prepared audio bus.
#[derive(Clone, Copy, Debug, Default)]
pub struct SummingBus;

impl AudioProcessor for SummingBus {
    fn process(&mut self, _block: AudioProcessBlock<'_>) -> Result<()> {
        Err(routing_error(
            "sum_bus",
            "audio bus requires prepared multi-input processing",
        ))
    }

    fn process_inputs(
        &mut self,
        block: AudioProcessBlock<'_>,
        inputs: crate::graph::AudioProcessInputs<'_>,
    ) -> Result<()> {
        if inputs.is_empty() {
            return Err(routing_error(
                "sum_bus",
                "audio bus requires at least one connected input",
            ));
        }

        block.output.fill(0.0);
        for input in inputs {
            debug_assert_eq!(input.layout(), block.output_layout);
            debug_assert_eq!(input.samples().len(), block.output.len());
            for (output, sample) in block.output.iter_mut().zip(input.samples()) {
                *output += sample;
                if !output.is_finite() {
                    return Err(routing_error(
                        "sum_bus",
                        "audio bus sum produced a non-finite sample",
                    ));
                }
            }
        }
        Ok(())
    }
}

fn routing_error(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

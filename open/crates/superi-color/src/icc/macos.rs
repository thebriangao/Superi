//! CoreGraphics active-display ICC discovery.

#![allow(unsafe_code)]

use std::sync::Arc;

use objc2_core_graphics::{
    CGColorSpace, CGDisplayCopyColorSpace, CGDisplayIsBuiltin, CGError, CGGetActiveDisplayList,
    CGMainDisplayID,
};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use super::{DisplayProfileObservation, MonitorId, COMPONENT, MAX_ACTIVE_DISPLAYS};

pub(super) fn discover() -> Result<Vec<DisplayProfileObservation>> {
    let requested_count = query_display_count()?;
    let requested_count = checked_display_count(requested_count)?;
    let mut display_ids = vec![0_u32; requested_count];
    let mut filled_count = 0_u32;
    let result = unsafe {
        // SAFETY: `display_ids` owns `requested_count` initialized entries,
        // the passed maximum is exactly that length, and `filled_count` is
        // a live out pointer for the duration of the CoreGraphics call.
        CGGetActiveDisplayList(
            requested_count as u32,
            display_ids.as_mut_ptr(),
            &mut filled_count,
        )
    };
    if result != CGError::Success {
        return Err(core_graphics_error(
            "enumerate_active_displays",
            result,
            "CoreGraphics could not enumerate active displays",
        ));
    }

    let filled_count = checked_display_count(filled_count)?;
    let confirmed_count = checked_display_count(query_display_count()?)?;
    if filled_count != requested_count || confirmed_count != filled_count {
        return Err(Error::new(
            ErrorCategory::Conflict,
            Recoverability::Retryable,
            "active display set changed during CoreGraphics discovery",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "enumerate_active_displays")
                .with_field("requested_count", requested_count.to_string())
                .with_field("filled_count", filled_count.to_string())
                .with_field("confirmed_count", confirmed_count.to_string()),
        ));
    }

    let primary = CGMainDisplayID();
    display_ids
        .iter()
        .copied()
        .map(|display_id| {
            let built_in = CGDisplayIsBuiltin(display_id);
            let color_space = CGDisplayCopyColorSpace(display_id);
            let profile = CGColorSpace::icc_data(Some(&color_space))
                .map(|data| Arc::<[u8]>::from(data.to_vec()));
            let id = MonitorId::new(format!("macos-cgdisplay:{display_id}"))?;
            let name = if built_in {
                "Built-in Display".to_owned()
            } else {
                format!("Display {display_id}")
            };
            DisplayProfileObservation::new(id, name, display_id == primary, built_in, profile)
        })
        .collect()
}

fn query_display_count() -> Result<u32> {
    let mut display_count = 0_u32;
    let result = unsafe {
        // SAFETY: a zero maximum requests only the active-display count, so the
        // display list pointer is null. `display_count` remains a live out
        // pointer for the complete CoreGraphics call.
        CGGetActiveDisplayList(0, std::ptr::null_mut(), &mut display_count)
    };
    if result != CGError::Success {
        return Err(core_graphics_error(
            "query_active_display_count",
            result,
            "CoreGraphics could not query the active display count",
        ));
    }
    Ok(display_count)
}

fn checked_display_count(display_count: u32) -> Result<usize> {
    let display_count = usize::try_from(display_count).map_err(|_| {
        core_graphics_error(
            "validate_active_display_count",
            CGError::RangeCheck,
            "CoreGraphics returned an invalid display count",
        )
    })?;
    if display_count == 0 {
        return Err(Error::new(
            ErrorCategory::Unavailable,
            Recoverability::Retryable,
            "CoreGraphics reports no active displays",
        )
        .with_context(ErrorContext::new(
            COMPONENT,
            "validate_active_display_count",
        )));
    }
    if display_count > MAX_ACTIVE_DISPLAYS {
        return Err(Error::new(
            ErrorCategory::ResourceExhausted,
            Recoverability::Retryable,
            "active macOS display count exceeds the fixed discovery limit",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "validate_active_display_count")
                .with_field("display_count", display_count.to_string())
                .with_field("display_limit", MAX_ACTIVE_DISPLAYS.to_string()),
        ));
    }
    Ok(display_count)
}

fn core_graphics_error(operation: &'static str, code: CGError, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        message,
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation).with_field("cg_error", code.0.to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_count_query_rejects_truncation_before_allocating() {
        assert_eq!(checked_display_count(1).unwrap(), 1);
        let error = checked_display_count((MAX_ACTIVE_DISPLAYS + 1) as u32).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
    }
}

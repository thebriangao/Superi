//! Original normalized icon registry for the native interface foundation.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Result, UiError};

/// Original geometry primitive inside a 24 by 24 view box.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IconPrimitive {
    Segment { from: [f32; 2], to: [f32; 2] },
    Polyline { points: Vec<[f32; 2]> },
    Polygon { points: Vec<[f32; 2]> },
    Circle { center: [f32; 2], radius: f32 },
}

/// Versioned original icon definition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IconDefinition {
    name: String,
    version: String,
    meaning: String,
    category: String,
    optical_inset: f32,
    stroke_width: f32,
    primitives: Vec<IconPrimitive>,
}

impl IconDefinition {
    /// Returns stable registry name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns normalized source geometry.
    #[must_use]
    pub fn primitives(&self) -> &[IconPrimitive] {
        &self.primitives
    }

    /// Returns optical inset.
    #[must_use]
    pub const fn optical_inset(&self) -> f32 {
        self.optical_inset
    }

    /// Returns normalized stroke width.
    #[must_use]
    pub const fn stroke_width(&self) -> f32 {
        self.stroke_width
    }

    /// Returns a deterministic source-geometry hash.
    #[must_use]
    pub fn geometry_hash(&self) -> String {
        let bytes = serde_json::to_vec(&self.primitives)
            .expect("icon primitives always serialize deterministically");
        format!("{:x}", Sha256::digest(bytes))
    }
}

/// Validated icon registry independent of atlas position.
#[derive(Clone, Debug)]
pub struct IconRegistry {
    version: &'static str,
    icons: BTreeMap<String, IconDefinition>,
}

impl IconRegistry {
    /// Returns the neutral icon seed needed by the scaffold diagnostic.
    #[must_use]
    pub fn foundation() -> Self {
        let mut icons = BTreeMap::new();
        for icon in [
            icon(
                "foundation.scene",
                "Identify one bounded retained scene",
                vec![
                    IconPrimitive::Polyline {
                        points: vec![[9.0, 5.0], [5.0, 5.0], [5.0, 9.0]],
                    },
                    IconPrimitive::Polyline {
                        points: vec![[15.0, 5.0], [19.0, 5.0], [19.0, 9.0]],
                    },
                    IconPrimitive::Polyline {
                        points: vec![[5.0, 15.0], [5.0, 19.0], [9.0, 19.0]],
                    },
                    IconPrimitive::Polyline {
                        points: vec![[19.0, 15.0], [19.0, 19.0], [15.0, 19.0]],
                    },
                    IconPrimitive::Polygon {
                        points: vec![[10.0, 10.0], [14.0, 10.0], [14.0, 14.0], [10.0, 14.0]],
                    },
                ],
            ),
            icon(
                "foundation.input",
                "Converge normalized input on one retained target",
                vec![
                    IconPrimitive::Segment {
                        from: [4.0, 6.0],
                        to: [9.0, 10.0],
                    },
                    IconPrimitive::Segment {
                        from: [4.0, 18.0],
                        to: [9.0, 14.0],
                    },
                    IconPrimitive::Segment {
                        from: [20.0, 12.0],
                        to: [15.0, 12.0],
                    },
                    IconPrimitive::Circle {
                        center: [12.0, 12.0],
                        radius: 3.0,
                    },
                ],
            ),
            icon(
                "foundation.semantics",
                "Project stable retained identity into one semantic tree",
                vec![
                    IconPrimitive::Circle {
                        center: [12.0, 5.0],
                        radius: 2.0,
                    },
                    IconPrimitive::Segment {
                        from: [12.0, 7.0],
                        to: [12.0, 11.0],
                    },
                    IconPrimitive::Segment {
                        from: [6.0, 11.0],
                        to: [18.0, 11.0],
                    },
                    IconPrimitive::Segment {
                        from: [6.0, 11.0],
                        to: [6.0, 16.0],
                    },
                    IconPrimitive::Segment {
                        from: [18.0, 11.0],
                        to: [18.0, 16.0],
                    },
                    IconPrimitive::Circle {
                        center: [6.0, 18.0],
                        radius: 2.0,
                    },
                    IconPrimitive::Circle {
                        center: [18.0, 18.0],
                        radius: 2.0,
                    },
                ],
            ),
            icon(
                "foundation.capture",
                "Bound a deterministic private render sample",
                vec![
                    IconPrimitive::Polyline {
                        points: vec![[10.0, 5.0], [5.0, 5.0], [5.0, 10.0]],
                    },
                    IconPrimitive::Polyline {
                        points: vec![[14.0, 5.0], [19.0, 5.0], [19.0, 10.0]],
                    },
                    IconPrimitive::Polyline {
                        points: vec![[5.0, 14.0], [5.0, 19.0], [10.0, 19.0]],
                    },
                    IconPrimitive::Polyline {
                        points: vec![[19.0, 14.0], [19.0, 19.0], [14.0, 19.0]],
                    },
                    IconPrimitive::Circle {
                        center: [12.0, 12.0],
                        radius: 2.0,
                    },
                ],
            ),
        ] {
            icons.insert(icon.name.clone(), icon);
        }
        Self {
            version: "1.0.0",
            icons,
        }
    }

    /// Returns registry version.
    #[must_use]
    pub const fn version(&self) -> &str {
        self.version
    }

    /// Resolves one icon by semantic identity.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&IconDefinition> {
        self.icons.get(name)
    }

    /// Returns all definitions in stable name order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &IconDefinition> {
        self.icons.values()
    }

    /// Finds geometry collisions.
    #[must_use]
    pub fn duplicates(&self) -> Vec<(String, String)> {
        let mut seen = BTreeMap::<String, String>::new();
        let mut duplicates = Vec::new();
        for definition in self.icons.values() {
            let hash = definition.geometry_hash();
            if let Some(previous) = seen.insert(hash, definition.name.clone()) {
                duplicates.push((previous, definition.name.clone()));
            }
        }
        duplicates
    }

    /// Validates names, versions, geometry, and collisions.
    pub fn validate(&self) -> Result<()> {
        let mut meanings = BTreeSet::new();
        for (name, definition) in &self.icons {
            if name != &definition.name
                || !valid_name(name)
                || definition.version != "1.0.0"
                || definition.meaning.trim().is_empty()
                || definition.category != "foundation"
                || !(0.0..=6.0).contains(&definition.optical_inset)
                || !(0.5..=4.0).contains(&definition.stroke_width)
                || definition.primitives.is_empty()
            {
                return Err(UiError::Invalid(format!(
                    "icon `{name}` has invalid registry metadata"
                )));
            }
            if !meanings.insert(definition.meaning.as_str()) {
                return Err(UiError::Invalid(format!(
                    "icon `{name}` duplicates another semantic meaning"
                )));
            }
            for primitive in &definition.primitives {
                validate_primitive(name, primitive)?;
            }
        }
        if let Some((left, right)) = self.duplicates().first() {
            return Err(UiError::Invalid(format!(
                "icons `{left}` and `{right}` have duplicate source geometry"
            )));
        }
        Ok(())
    }

    /// Hashes the complete stable registry.
    #[must_use]
    pub fn registry_hash(&self) -> String {
        let bytes = serde_json::to_vec(&self.icons)
            .expect("icon registry always serializes deterministically");
        format!("{:x}", Sha256::digest(bytes))
    }
}

fn icon(name: &str, meaning: &str, primitives: Vec<IconPrimitive>) -> IconDefinition {
    IconDefinition {
        name: name.to_owned(),
        version: "1.0.0".to_owned(),
        meaning: meaning.to_owned(),
        category: "foundation".to_owned(),
        optical_inset: 2.0,
        stroke_width: 1.7,
        primitives,
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.ends_with('.')
        && !name.contains("..")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'.')
}

fn validate_primitive(name: &str, primitive: &IconPrimitive) -> Result<()> {
    let points: Vec<[f32; 2]> = match primitive {
        IconPrimitive::Segment { from, to } => vec![*from, *to],
        IconPrimitive::Polyline { points } | IconPrimitive::Polygon { points } => points.clone(),
        IconPrimitive::Circle { center, radius } => {
            if !radius.is_finite() || *radius <= 0.0 || *radius > 12.0 {
                return Err(UiError::Invalid(format!(
                    "icon `{name}` has an invalid circle"
                )));
            }
            vec![*center]
        }
    };
    if points.is_empty()
        || points
            .iter()
            .flatten()
            .any(|value| !value.is_finite() || !(0.0..=24.0).contains(value))
    {
        return Err(UiError::Invalid(format!(
            "icon `{name}` leaves its 24 by 24 view box"
        )));
    }
    Ok(())
}

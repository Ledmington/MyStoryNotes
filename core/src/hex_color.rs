//! Serializes an `[u8; 3]` RGB color as a `"#rrggbb"` hex string instead of a 3-element array of
//! channel numbers, so a hand-edited save file has one short, recognizable token per color
//! instead of three. Used via `#[serde(with = "crate::hex_color")]` on every persisted color
//! field, in both [`crate::project`] and [`crate::settings`]. Deserializing still accepts the
//! older array form too, so a save file written before this format change keeps opening.

use std::fmt;

use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserializer, Serializer};

pub(crate) fn serialize<S>(color: &[u8; 3], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&format!(
        "#{:02x}{:02x}{:02x}",
        color[0], color[1], color[2]
    ))
}

pub(crate) fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 3], D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(ColorVisitor)
}

struct ColorVisitor;

impl<'de> Visitor<'de> for ColorVisitor {
    type Value = [u8; 3];

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(
            "a \"#rrggbb\" hex color string, or (from a save file written before this format) \
             a 3-element array of channel numbers",
        )
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        parse(value).ok_or_else(|| {
            E::custom(format!(
                "invalid color {value:?}, expected e.g. \"#a1b2c3\""
            ))
        })
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut channels = [0u8; 3];
        for (index, channel) in channels.iter_mut().enumerate() {
            *channel = seq
                .next_element()?
                .ok_or_else(|| de::Error::invalid_length(index, &self))?;
        }
        Ok(channels)
    }
}

fn parse(text: &str) -> Option<[u8; 3]> {
    let hex = text.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let channel = |range: std::ops::Range<usize>| u8::from_str_radix(&hex[range], 16).ok();
    Some([channel(0..2)?, channel(2..4)?, channel(4..6)?])
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Wrapper {
        #[serde(with = "super")]
        color: [u8; 3],
    }

    #[test]
    fn serializes_as_a_hex_string() {
        let text = toml::to_string(&Wrapper {
            color: [0x1b, 0x9c, 0xd6],
        })
        .unwrap();

        assert_eq!(text, "color = \"#1b9cd6\"\n");
    }

    #[test]
    fn deserializes_a_hex_string() {
        let wrapper: Wrapper = toml::from_str("color = \"#1b9cd6\"").unwrap();

        assert_eq!(
            wrapper,
            Wrapper {
                color: [0x1b, 0x9c, 0xd6]
            }
        );
    }

    #[test]
    fn deserializes_the_legacy_array_form_for_backward_compatibility() {
        let wrapper: Wrapper = toml::from_str("color = [27, 156, 214]").unwrap();

        assert_eq!(
            wrapper,
            Wrapper {
                color: [27, 156, 214]
            }
        );
    }

    #[test]
    fn rejects_a_malformed_hex_string() {
        assert!(toml::from_str::<Wrapper>("color = \"not-a-color\"").is_err());
    }
}

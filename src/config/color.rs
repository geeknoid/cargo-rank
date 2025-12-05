use core::fmt::{Formatter as FmtFormatter, Result as FmtResult};
use palette::Srgb;
use serde::{Deserialize, Serialize};

/// A color that can be deserialized from:
/// - Hex strings: "#FF0000", "#F00", "FF0000", "F00"
/// - Named colors: "red", "green", "blue", "orange", etc. (SVG/CSS3 color names)
/// - RGB structs: { red: 255, green: 0, blue: 0 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(pub Srgb<u8>);

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        // Try to deserialize as a string first (hex color)
        struct ColorVisitor;

        impl<'de> serde::de::Visitor<'de> for ColorVisitor {
            type Value = Color;

            fn expecting(&self, formatter: &mut FmtFormatter) -> FmtResult {
                formatter.write_str("a hex color string like \"#FF0000\", a named color like \"red\", or an RGB struct")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                // Try parsing as hex color first
                if let Ok(color) = v.parse::<Srgb<u8>>() {
                    return Ok(Color(color));
                }

                // Try parsing as named color
                palette::named::from_str(v)
                    .map(|named_color| Color(Srgb::from_format(named_color)))
                    .ok_or_else(|| {
                        E::custom(format!(
                            "invalid color: '{v}' (must be a hex color like '#FF0000' or a named color like 'red')"
                        ))
                    })
            }

            fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
            where
                M: serde::de::MapAccess<'de>,
            {
                Srgb::<u8>::deserialize(serde::de::value::MapAccessDeserializer::new(map)).map(Color)
            }
        }

        deserializer.deserialize_any(ColorVisitor)
    }
}

impl Serialize for Color {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

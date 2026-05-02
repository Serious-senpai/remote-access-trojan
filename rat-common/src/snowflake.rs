use std::borrow::Cow;
use std::fmt::Display;
use std::num::ParseIntError;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fmt, iter};

use poem_openapi::registry::{MetaSchema, MetaSchemaRef};
use poem_openapi::types::{ParseError, ParseFromJSON, ParseResult, ToJSON, Type};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SnowflakeId(u128);

static _COUNTER: AtomicU32 = AtomicU32::new(0);

impl SnowflakeId {
    const _TAIL_BITS: u32 = 32;
    const _TIMESTAMP_BITS: u32 = 128 - Self::_TAIL_BITS;

    /// Unix timestamp for 2026-01-01T00:00:00Z
    const _EPOCH_2026_MS: u128 = 1767225600000;

    pub fn new() -> Self {
        let elapsed_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis()
            - Self::_EPOCH_2026_MS;

        let time_part = elapsed_ms << Self::_TAIL_BITS;
        let counter = _COUNTER.fetch_add(1, Ordering::Relaxed);

        Self(time_part | u128::from(counter))
    }
}

impl Default for SnowflakeId {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for SnowflakeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<'de> Deserialize<'de> for SnowflakeId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let value = s.parse::<u128>().map_err(de::Error::custom)?;
        Ok(Self(value))
    }
}

impl Serialize for SnowflakeId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl TryFrom<String> for SnowflakeId {
    type Error = ParseIntError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse::<u128>().map(SnowflakeId)
    }
}

// ChatGPT-generated trait implementations for OpenAPI integration
// TODO: Clean this mess

impl Type for SnowflakeId {
    const IS_REQUIRED: bool = true;

    type RawValueType = Self;
    type RawElementValueType = Self;

    fn name() -> Cow<'static, str> {
        "SnowflakeId".into()
    }

    fn schema_ref() -> MetaSchemaRef {
        MetaSchemaRef::Inline(Box::new(MetaSchema {
            ty: "string",
            format: Some("snowflake"),
            ..MetaSchema::ANY
        }))
    }

    fn as_raw_value(&self) -> Option<&Self::RawValueType> {
        Some(self)
    }

    fn raw_element_iter<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = &'a Self::RawElementValueType> + 'a> {
        Box::new(iter::once(self))
    }
}

impl ParseFromJSON for SnowflakeId {
    fn parse_from_json(value: Option<serde_json::Value>) -> ParseResult<Self> {
        let value = value.ok_or_else(ParseError::expected_input)?;

        match value {
            serde_json::Value::String(s) => {
                let parsed = s
                    .parse::<u128>()
                    .map_err(|e| ParseError::custom(e.to_string()))?;
                Ok(Self(parsed))
            }
            _ => Err(ParseError::expected_type(value)),
        }
    }
}

impl ToJSON for SnowflakeId {
    fn to_json(&self) -> Option<serde_json::Value> {
        Some(serde_json::Value::String(self.0.to_string()))
    }
}

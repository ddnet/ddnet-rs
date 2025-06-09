use std::{
    collections::HashMap,
    fmt::Display,
    num::{ParseFloatError, ParseIntError},
    str::ParseBoolError,
    time::Duration,
};

use anyhow::anyhow;
use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;

use crate::types::ConfRgb;

pub use serde_json;

#[derive(Error, Debug)]
pub enum ConfigFromStrPathErr {
    #[error("Expected end of path, but found {0}")]
    EndOfPath(String),
    #[error("Value {path:?} not found in the allowed names: {allowed_paths:?}")]
    PathNotFound {
        path: String,
        allowed_paths: Vec<String>,
    },
    #[error("Failed to parse value: {0}")]
    ParsingErr(String),
    #[error("Validation failed: {0}")]
    ValidationError(String),
    // a fatal error, but not on the highest level
    #[error("{0}")]
    FatalErr(String),
}

#[derive(Error, Debug)]
pub enum ConfigFromStrErr {
    #[error("{0}")]
    PathErr(ConfigFromStrPathErr),
    #[error("{0}")]
    FatalErr(String),
}

#[derive(Debug, Clone)]
pub enum ConfigValue {
    Boolean,
    Int {
        min: i64,
        max: u64,
    },
    Float {
        min: f64,
        max: f64,
    },
    String {
        min_length: usize,
        max_length: usize,
    },
    /// Rgb color
    Color,
    StringOfList {
        allowed_values: Vec<String>,
    },
    Array {
        val_ty: Box<ConfigValue>,
        min_length: usize,
        max_length: usize,
    },
    /// Basically { "name": any, "name2": any }, useful for e.g. a hashmap.
    /// However numbers as first letters are not allowed!
    JsonLikeRecord {
        val_ty: Box<ConfigValue>,
    },
    /// A container of console variables.
    Struct {
        attributes: Vec<ConfigValueAttr>,
        aliases: Vec<(String, String)>,
        name: String,
    },
}

#[derive(Debug, Clone)]
pub struct ConfigValueAttr {
    pub name: String,
    pub val: ConfigValue,
    pub description: String,
}

#[derive(Debug, Default, Clone)]
pub enum ConfigFromStrOperation {
    #[default]
    Set,
    Push,
    Pop,
    Rem,
    Reset,
    /// Allow arbitrary operations if needed somewhere
    Other(Vec<u8>),
}

pub trait ConfigInterface {
    /// structs might overwrite certain values of
    /// the config values attributes
    fn conf_value() -> ConfigValue
    where
        Self: Sized;

    /// sets the config value from a string
    /// takes path. which is the full path separated by `.`
    /// an optional modifier, which is only intersting for internal logic (e.g. array indices, hashmap indices)
    /// and optionally the value represented in a string
    /// always returns the current value as a string representation
    fn try_set_from_str(
        &mut self,
        path: String,
        modifier: Option<String>,
        val: Option<String>,
        conf_val: Option<&ConfigValue>,
        op: ConfigFromStrOperation,
    ) -> anyhow::Result<String, ConfigFromStrErr>;
}

impl ConfigInterface for String {
    fn conf_value() -> ConfigValue {
        ConfigValue::String {
            min_length: 0,
            max_length: usize::MAX,
        }
    }

    fn try_set_from_str(
        &mut self,
        path: String,
        _modifier: Option<String>,
        val: Option<String>,
        conf_val: Option<&ConfigValue>,
        _op: ConfigFromStrOperation,
    ) -> anyhow::Result<String, ConfigFromStrErr> {
        if path.is_empty() {
            if let Some(val) = val {
                // validate
                if let Err((min, max)) = if let Some(ConfigValue::String {
                    min_length,
                    max_length,
                }) = conf_val
                {
                    let char_count = val.chars().count();
                    if char_count < *min_length {
                        Err((*min_length, *max_length))
                    } else {
                        (char_count < *max_length)
                            .then_some(())
                            .ok_or((*min_length, *max_length))
                    }
                } else {
                    Ok(())
                } {
                    return Err(ConfigFromStrErr::PathErr(
                        ConfigFromStrPathErr::ValidationError(format!(
                            "The min/max length of the string was reached ({min}/{max})"
                        )),
                    ));
                }
                *self = val;
            }
            Ok(self.clone())
        } else {
            Err(ConfigFromStrErr::PathErr(ConfigFromStrPathErr::EndOfPath(
                path,
            )))
        }
    }
}

fn validate_numerical<I: PartialOrd + Display>(
    min: I,
    max: I,
    val: I,
) -> anyhow::Result<(), ConfigFromStrErr> {
    if min > val || max < val {
        Err(ConfigFromStrErr::PathErr(
            ConfigFromStrPathErr::ValidationError(format!(
                "Numerical value out of allowed range: {val} not in [{min}, {max}]"
            )),
        ))
    } else {
        Ok(())
    }
}
fn validate_int<I: PartialOrd + Display + Into<i128>>(
    conf_val: &Option<&ConfigValue>,
    val: I,
) -> anyhow::Result<(), ConfigFromStrErr> {
    if let Some(ConfigValue::Int { min, max }) = conf_val {
        validate_numerical(*min as i128, *max as i128, val.into())
    } else {
        Ok(())
    }
}
fn validate_float<I: PartialOrd + Display + Into<f64>>(
    conf_val: &Option<&ConfigValue>,
    val: I,
) -> anyhow::Result<(), ConfigFromStrErr> {
    if let Some(ConfigValue::Float { min, max }) = conf_val {
        validate_numerical(*min, *max, val.into())
    } else {
        Ok(())
    }
}

impl ConfigInterface for Duration {
    fn conf_value() -> ConfigValue {
        ConfigValue::Int {
            min: 0,
            max: u64::MAX,
        }
    }

    fn try_set_from_str(
        &mut self,
        path: String,
        _modifier: Option<String>,
        val: Option<String>,
        conf_val: Option<&ConfigValue>,
        _op: ConfigFromStrOperation,
    ) -> anyhow::Result<String, ConfigFromStrErr> {
        if path.is_empty() {
            if let Some(val) = val {
                let v = val.parse().map_err(|err: ParseIntError| {
                    ConfigFromStrErr::PathErr(ConfigFromStrPathErr::ParsingErr(err.to_string()))
                })?;
                validate_int(&conf_val, v)?;
                *self = Duration::from_millis(v);
            }
            Ok(self.as_millis().to_string())
        } else {
            Err(ConfigFromStrErr::PathErr(ConfigFromStrPathErr::EndOfPath(
                path,
            )))
        }
    }
}

impl<V: Default + ConfigInterface + DeserializeOwned + Serialize> ConfigInterface for Vec<V> {
    fn conf_value() -> ConfigValue {
        let val_ty = V::conf_value();
        // TODO: make this compile time assert as soon as rust supports it
        assert!(!matches!(
            val_ty,
            ConfigValue::Array { .. } | ConfigValue::JsonLikeRecord { .. }
        ), "Currently arrays in arrays or records in arrays or the other way around are not allowed");
        ConfigValue::Array {
            val_ty: Box::new(val_ty),

            min_length: 0,
            max_length: usize::MAX,
        }
    }

    fn try_set_from_str(
        &mut self,
        path: String,
        modifier: Option<String>,
        val: Option<String>,
        conf_val: Option<&ConfigValue>,
        op: ConfigFromStrOperation,
    ) -> anyhow::Result<String, ConfigFromStrErr> {
        if matches!(op, ConfigFromStrOperation::Push) {
            if conf_val.is_none()
                || conf_val.is_some_and(|v| {
                    if let ConfigValue::Array { max_length, .. } = v {
                        *max_length > self.len() + 1
                    } else {
                        false
                    }
                })
            {
                self.push(Default::default());
            } else {
                return Err(ConfigFromStrErr::PathErr(
                    ConfigFromStrPathErr::ValidationError(
                        "The max length of the array is reached".to_string(),
                    ),
                ));
            }
            Ok(serde_json::to_string(self).map_err(|err| {
                ConfigFromStrErr::FatalErr(format!("Could not serialize current value: {err}"))
            })?)
        } else if matches!(op, ConfigFromStrOperation::Pop) {
            if conf_val.is_none()
                || conf_val.is_some_and(|v| {
                    if let ConfigValue::Array { min_length, .. } = v {
                        !self.is_empty() && *min_length < self.len()
                    } else {
                        false
                    }
                })
            {
                self.pop();
            } else {
                return Err(ConfigFromStrErr::PathErr(
                    ConfigFromStrPathErr::ValidationError(
                        "The min length of the array is reached".to_string(),
                    ),
                ));
            }
            Ok(serde_json::to_string(self).map_err(|err| {
                ConfigFromStrErr::FatalErr(format!("Could not serialize current value: {err}"))
            })?)
        } else if let Some(modifier) = modifier {
            let index: usize = modifier.parse().map_err(|err| {
                ConfigFromStrErr::PathErr(ConfigFromStrPathErr::ParsingErr(format!(
                    "index not parsable: {err}"
                )))
            })?;
            let v = self.get_mut(index).ok_or_else(|| {
                ConfigFromStrErr::PathErr(ConfigFromStrPathErr::FatalErr(
                    "value with that index does not exist, use `push <var>` to add new entry"
                        .into(),
                ))
            })?;

            // If reset operation, then take the default of the generic argument.
            // If the default fails, pass reset operation to the next value.
            let (val, op) = if matches!(op, ConfigFromStrOperation::Reset) {
                let val = V::default()
                    .try_set_from_str(path.clone(), None, None, None, Default::default())
                    .ok();

                let op = if val.is_some() {
                    Default::default()
                } else {
                    ConfigFromStrOperation::Reset
                };
                (val, op)
            } else {
                (val, Default::default())
            };

            v.try_set_from_str(path, None, val, None, op)
        } else if path.is_empty() {
            if matches!(op, ConfigFromStrOperation::Reset) {
                // This usually should never happen since the struct resets the array,
                // but it also cannot hurt.
                *self = Vec::new();
            } else if let Some(val) = val {
                *self = serde_json::from_str(&val).map_err(|err: serde_json::Error| {
                    ConfigFromStrErr::PathErr(ConfigFromStrPathErr::ParsingErr(err.to_string()))
                })?;
            }
            Ok(serde_json::to_string(self).map_err(|err| {
                ConfigFromStrErr::FatalErr(format!("Could not serialize current value: {err}"))
            })?)
        } else {
            Err(ConfigFromStrErr::PathErr(ConfigFromStrPathErr::FatalErr(
                "expected [index]... or nothing, but found another path".into(),
            )))
        }
    }
}

impl<T> crate::traits::ConfigInterface for HashMap<String, T>
where
    T: Default + ConfigInterface + Serialize + DeserializeOwned,
{
    fn conf_value() -> crate::traits::ConfigValue {
        let val_ty = T::conf_value();
        // TODO: make this compile time assert as soon as rust supports it
        assert!(!matches!(
            val_ty,
            ConfigValue::Array { .. } | ConfigValue::JsonLikeRecord { .. }
        ), "Currently arrays in arrays or records in arrays or the other way around are not allowed");
        ConfigValue::JsonLikeRecord {
            val_ty: Box::new(val_ty),
        }
    }

    fn try_set_from_str(
        &mut self,
        path: String,
        modifier: Option<String>,
        val: Option<String>,
        _conf_val: Option<&ConfigValue>,
        op: ConfigFromStrOperation,
    ) -> anyhow::Result<String, ConfigFromStrErr> {
        if matches!(op, ConfigFromStrOperation::Rem) {
            if let Some(modifier) = modifier {
                let index: &String = &modifier;
                self.remove(index);
            }
            Ok(serde_json::to_string(self).map_err(|err| {
                ConfigFromStrErr::FatalErr(format!("Could not serialize current value: {err}"))
            })?)
        } else if let Some(modifier) = modifier {
            let index: &String = &modifier;
            // if value is Some, assume that the assign on the child will succeed
            // a.k.a. the user at least wanted to assign a value
            if !matches!(op, ConfigFromStrOperation::Reset)
                && !self.contains_key(index)
                && val.is_some()
            {
                self.insert(index.clone(), Default::default());
            }

            let v = self.get_mut(index).ok_or_else(|| {
                ConfigFromStrErr::PathErr(ConfigFromStrPathErr::FatalErr(
                    "value not yet assigned".into(),
                ))
            })?;

            // If reset operation, then take the default of the generic argument.
            // If the default fails, pass reset operation to the next value.
            let (val, op) = if matches!(op, ConfigFromStrOperation::Reset) {
                let val = T::default()
                    .try_set_from_str(path.clone(), None, None, None, Default::default())
                    .ok();

                let op = if val.is_some() {
                    Default::default()
                } else {
                    ConfigFromStrOperation::Reset
                };
                (val, op)
            } else {
                (val, Default::default())
            };

            v.try_set_from_str(path, None, val, None, op)
        } else if path.is_empty() {
            if matches!(op, ConfigFromStrOperation::Reset) {
                // This usually should never happen since the struct resets the array,
                // but it also cannot hurt.
                *self = Default::default();
            } else if let Some(val) = val {
                *self = serde_json::from_str(&val).map_err(|err: serde_json::Error| {
                    ConfigFromStrErr::PathErr(ConfigFromStrPathErr::ParsingErr(err.to_string()))
                })?;
            }

            Ok(serde_json::to_string(self).map_err(|err| {
                ConfigFromStrErr::FatalErr(format!("Could not serialize current value: {err}"))
            })?)
        } else {
            Err(ConfigFromStrErr::PathErr(ConfigFromStrPathErr::FatalErr(
                "expected [key]... or nothing, but found another path".into(),
            )))
        }
    }
}

impl ConfigInterface for bool {
    fn conf_value() -> ConfigValue {
        ConfigValue::Boolean
    }

    fn try_set_from_str(
        &mut self,
        path: String,
        _modifier: Option<String>,
        val: Option<String>,
        _conf_val: Option<&ConfigValue>,
        _op: ConfigFromStrOperation,
    ) -> anyhow::Result<String, ConfigFromStrErr> {
        if path.is_empty() {
            if let Some(val) = val {
                *self = val
                    .parse()
                    .map_err(|err: ParseBoolError| {
                        ConfigFromStrErr::PathErr(ConfigFromStrPathErr::ParsingErr(err.to_string()))
                    })
                    .or_else(|err_bool| {
                        val.parse::<u8>()
                            .map(|v| v == 1)
                            .map_err(|err: ParseIntError| {
                                ConfigFromStrErr::PathErr(ConfigFromStrPathErr::ParsingErr(
                                    format!("{err}. {err_bool}"),
                                ))
                            })
                    })?;
            }
            Ok(self.to_string())
        } else {
            Err(ConfigFromStrErr::PathErr(ConfigFromStrPathErr::EndOfPath(
                path,
            )))
        }
    }
}

impl ConfigInterface for u8 {
    fn conf_value() -> ConfigValue {
        ConfigValue::Int {
            min: Self::MIN as i64,
            max: Self::MAX as u64,
        }
    }

    fn try_set_from_str(
        &mut self,
        path: String,
        _modifier: Option<String>,
        val: Option<String>,
        conf_val: Option<&ConfigValue>,
        _op: ConfigFromStrOperation,
    ) -> anyhow::Result<String, ConfigFromStrErr> {
        if path.is_empty() {
            if let Some(val) = val {
                let v = val.parse().map_err(|err: ParseIntError| {
                    ConfigFromStrErr::PathErr(ConfigFromStrPathErr::ParsingErr(err.to_string()))
                })?;
                validate_int(&conf_val, v)?;
                *self = v;
            }
            Ok(self.to_string())
        } else {
            Err(ConfigFromStrErr::PathErr(ConfigFromStrPathErr::EndOfPath(
                path,
            )))
        }
    }
}

impl ConfigInterface for i8 {
    fn conf_value() -> ConfigValue {
        ConfigValue::Int {
            min: Self::MIN as i64,
            max: Self::MAX as u64,
        }
    }

    fn try_set_from_str(
        &mut self,
        path: String,
        _modifier: Option<String>,
        val: Option<String>,
        conf_val: Option<&ConfigValue>,
        _op: ConfigFromStrOperation,
    ) -> anyhow::Result<String, ConfigFromStrErr> {
        if path.is_empty() {
            if let Some(val) = val {
                let v = val.parse().map_err(|err: ParseIntError| {
                    ConfigFromStrErr::PathErr(ConfigFromStrPathErr::ParsingErr(err.to_string()))
                })?;
                validate_int(&conf_val, v)?;
                *self = v;
            }
            Ok(self.to_string())
        } else {
            Err(ConfigFromStrErr::PathErr(ConfigFromStrPathErr::EndOfPath(
                path,
            )))
        }
    }
}

impl ConfigInterface for u16 {
    fn conf_value() -> ConfigValue {
        ConfigValue::Int {
            min: Self::MIN as i64,
            max: Self::MAX as u64,
        }
    }

    fn try_set_from_str(
        &mut self,
        path: String,
        _modifier: Option<String>,
        val: Option<String>,
        conf_val: Option<&ConfigValue>,
        _op: ConfigFromStrOperation,
    ) -> anyhow::Result<String, ConfigFromStrErr> {
        if path.is_empty() {
            if let Some(val) = val {
                let v = val.parse().map_err(|err: ParseIntError| {
                    ConfigFromStrErr::PathErr(ConfigFromStrPathErr::ParsingErr(err.to_string()))
                })?;
                validate_int(&conf_val, v)?;
                *self = v;
            }
            Ok(self.to_string())
        } else {
            Err(ConfigFromStrErr::PathErr(ConfigFromStrPathErr::EndOfPath(
                path,
            )))
        }
    }
}

impl ConfigInterface for i16 {
    fn conf_value() -> ConfigValue {
        ConfigValue::Int {
            min: Self::MIN as i64,
            max: Self::MAX as u64,
        }
    }

    fn try_set_from_str(
        &mut self,
        path: String,
        _modifier: Option<String>,
        val: Option<String>,
        conf_val: Option<&ConfigValue>,
        _op: ConfigFromStrOperation,
    ) -> anyhow::Result<String, ConfigFromStrErr> {
        if path.is_empty() {
            if let Some(val) = val {
                let v = val.parse().map_err(|err: ParseIntError| {
                    ConfigFromStrErr::PathErr(ConfigFromStrPathErr::ParsingErr(err.to_string()))
                })?;
                validate_int(&conf_val, v)?;
                *self = v;
            }
            Ok(self.to_string())
        } else {
            Err(ConfigFromStrErr::PathErr(ConfigFromStrPathErr::EndOfPath(
                path,
            )))
        }
    }
}

impl ConfigInterface for u32 {
    fn conf_value() -> ConfigValue {
        ConfigValue::Int {
            min: Self::MIN as i64,
            max: Self::MAX as u64,
        }
    }

    fn try_set_from_str(
        &mut self,
        path: String,
        _modifier: Option<String>,
        val: Option<String>,
        conf_val: Option<&ConfigValue>,
        _op: ConfigFromStrOperation,
    ) -> anyhow::Result<String, ConfigFromStrErr> {
        if path.is_empty() {
            if let Some(val) = val {
                let v = val.parse().map_err(|err: ParseIntError| {
                    ConfigFromStrErr::PathErr(ConfigFromStrPathErr::ParsingErr(err.to_string()))
                })?;
                validate_int(&conf_val, v)?;
                *self = v;
            }
            Ok(self.to_string())
        } else {
            Err(ConfigFromStrErr::PathErr(ConfigFromStrPathErr::EndOfPath(
                path,
            )))
        }
    }
}

impl ConfigInterface for i32 {
    fn conf_value() -> ConfigValue {
        ConfigValue::Int {
            min: Self::MIN as i64,
            max: Self::MAX as u64,
        }
    }

    fn try_set_from_str(
        &mut self,
        path: String,
        _modifier: Option<String>,
        val: Option<String>,
        conf_val: Option<&ConfigValue>,
        _op: ConfigFromStrOperation,
    ) -> anyhow::Result<String, ConfigFromStrErr> {
        if path.is_empty() {
            if let Some(val) = val {
                let v = val.parse().map_err(|err: ParseIntError| {
                    ConfigFromStrErr::PathErr(ConfigFromStrPathErr::ParsingErr(err.to_string()))
                })?;
                validate_int(&conf_val, v)?;
                *self = v;
            }
            Ok(self.to_string())
        } else {
            Err(ConfigFromStrErr::PathErr(ConfigFromStrPathErr::EndOfPath(
                path,
            )))
        }
    }
}

impl ConfigInterface for u64 {
    fn conf_value() -> ConfigValue {
        ConfigValue::Int {
            min: Self::MIN as i64,
            max: Self::MAX,
        }
    }

    fn try_set_from_str(
        &mut self,
        path: String,
        _modifier: Option<String>,
        val: Option<String>,
        conf_val: Option<&ConfigValue>,
        _op: ConfigFromStrOperation,
    ) -> anyhow::Result<String, ConfigFromStrErr> {
        if path.is_empty() {
            if let Some(val) = val {
                let v = val.parse().map_err(|err: ParseIntError| {
                    ConfigFromStrErr::PathErr(ConfigFromStrPathErr::ParsingErr(err.to_string()))
                })?;
                validate_int(&conf_val, v)?;
                *self = v;
            }
            Ok(self.to_string())
        } else {
            Err(ConfigFromStrErr::PathErr(ConfigFromStrPathErr::EndOfPath(
                path,
            )))
        }
    }
}

impl ConfigInterface for i64 {
    fn conf_value() -> ConfigValue {
        ConfigValue::Int {
            min: Self::MIN,
            max: Self::MAX as u64,
        }
    }

    fn try_set_from_str(
        &mut self,
        path: String,
        _modifier: Option<String>,
        val: Option<String>,
        conf_val: Option<&ConfigValue>,
        _op: ConfigFromStrOperation,
    ) -> anyhow::Result<String, ConfigFromStrErr> {
        if path.is_empty() {
            if let Some(val) = val {
                let v = val.parse().map_err(|err: ParseIntError| {
                    ConfigFromStrErr::PathErr(ConfigFromStrPathErr::ParsingErr(err.to_string()))
                })?;
                validate_int(&conf_val, v)?;
                *self = v;
            }
            Ok(self.to_string())
        } else {
            Err(ConfigFromStrErr::PathErr(ConfigFromStrPathErr::EndOfPath(
                path,
            )))
        }
    }
}

impl ConfigInterface for f32 {
    fn conf_value() -> ConfigValue {
        ConfigValue::Float {
            min: Self::MIN as f64,
            max: Self::MAX as f64,
        }
    }

    fn try_set_from_str(
        &mut self,
        path: String,
        _modifier: Option<String>,
        val: Option<String>,
        conf_val: Option<&ConfigValue>,
        _op: ConfigFromStrOperation,
    ) -> anyhow::Result<String, ConfigFromStrErr> {
        if path.is_empty() {
            if let Some(val) = val {
                let v = val.parse().map_err(|err: ParseFloatError| {
                    ConfigFromStrErr::PathErr(ConfigFromStrPathErr::ParsingErr(err.to_string()))
                })?;
                validate_float(&conf_val, v)?;
                *self = v;
            }
            Ok(self.to_string())
        } else {
            Err(ConfigFromStrErr::PathErr(ConfigFromStrPathErr::EndOfPath(
                path,
            )))
        }
    }
}

impl ConfigInterface for f64 {
    fn conf_value() -> ConfigValue {
        ConfigValue::Float {
            min: Self::MIN,
            max: Self::MAX,
        }
    }

    fn try_set_from_str(
        &mut self,
        path: String,
        _modifier: Option<String>,
        val: Option<String>,
        conf_val: Option<&ConfigValue>,
        _op: ConfigFromStrOperation,
    ) -> anyhow::Result<String, ConfigFromStrErr> {
        if path.is_empty() {
            if let Some(val) = val {
                let v = val.parse().map_err(|err: ParseFloatError| {
                    ConfigFromStrErr::PathErr(ConfigFromStrPathErr::ParsingErr(err.to_string()))
                })?;
                validate_float(&conf_val, v)?;
                *self = v;
            }
            Ok(self.to_string())
        } else {
            Err(ConfigFromStrErr::PathErr(ConfigFromStrPathErr::EndOfPath(
                path,
            )))
        }
    }
}

impl<T: ConfigInterface + Default + ToString> ConfigInterface for Option<T> {
    fn conf_value() -> ConfigValue {
        T::conf_value()
    }

    fn try_set_from_str(
        &mut self,
        path: String,
        modifier: Option<String>,
        val: Option<String>,
        conf_val: Option<&ConfigValue>,
        op: ConfigFromStrOperation,
    ) -> anyhow::Result<String, ConfigFromStrErr> {
        if path.is_empty() {
            if matches!(op, ConfigFromStrOperation::Reset) {
                *self = None;
            } else if let Some(val_inner) = &val {
                if val_inner.is_empty() {
                    *self = None;
                } else {
                    let mut t = T::default();
                    let v = t.try_set_from_str(path, modifier, val, conf_val, op)?;
                    *self = Some(t);
                    return Ok(v);
                }
            }
            Ok(match self {
                Some(val) => val.to_string(),
                None => "[None]".to_string(),
            })
        } else {
            match self {
                Some(inner_val) => {
                    // If reset operation, then take the default of the generic argument.
                    // If the default fails, pass reset operation to the next value.
                    let (val, op) = if matches!(op, ConfigFromStrOperation::Reset) {
                        let val = T::default()
                            .try_set_from_str(
                                path.clone(),
                                modifier.clone(),
                                None,
                                conf_val,
                                Default::default(),
                            )
                            .ok();

                        let op = if val.is_some() {
                            Default::default()
                        } else {
                            ConfigFromStrOperation::Reset
                        };
                        (val, op)
                    } else {
                        (val, Default::default())
                    };

                    inner_val.try_set_from_str(path, modifier, val, conf_val, op)
                }
                None => {
                    if matches!(op, ConfigFromStrOperation::Reset) {
                        Err(ConfigFromStrErr::FatalErr(
                            "Tried to reset the inner value of an option that was none".to_string(),
                        ))
                    } else {
                        let mut t = T::default();
                        let res = t.try_set_from_str(path, modifier, val, conf_val, op)?;

                        *self = Some(t);

                        Ok(res)
                    }
                }
            }
        }
    }
}

impl ConfigInterface for ConfRgb {
    fn conf_value() -> ConfigValue {
        ConfigValue::Color
    }

    fn try_set_from_str(
        &mut self,
        path: String,
        _modifier: Option<String>,
        val: Option<String>,
        _conf_val: Option<&ConfigValue>,
        _op: ConfigFromStrOperation,
    ) -> anyhow::Result<String, ConfigFromStrErr> {
        if path.is_empty() {
            if let Some(val) = val {
                *self = ConfRgb::from_html_color_code(&val)
                    .or_else(|err_html| {
                        ConfRgb::from_css_rgb_fn(&val).or_else(|err_css| {
                            ConfRgb::from_display(&val).map_err(|err| {
                                anyhow!("no html, css or display like color tag found: {err_html}. {err_css}. {err}")
                            })
                        })
                    })
                    .map_err(|err| ConfigFromStrErr::FatalErr(err.to_string()))?;
            }
            Ok(self.to_string())
        } else {
            Err(ConfigFromStrErr::PathErr(ConfigFromStrPathErr::EndOfPath(
                path,
            )))
        }
    }
}

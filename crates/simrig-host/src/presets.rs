//! Presets: named sets of values ("GT3", "Drift") applied from the app and saved into the
//! drivers, so they stay active with the app closed.
//!
//! ```toml
//! name = "GT3"
//!
//! [wheel_base]                          # every wheel base in the rig, any brand
//! rotation = 540
//!
//! [pedals]
//! "brake.curve" = [[0, 0], [0.5, 0.35], [1, 1]]
//!
//! [component."moza/usb-346e-0003/pedals"]     # only this component
//! "brake.max_force" = 60
//!
//! [custom.fanatec]                      # custom parameters of a plugin's components
//! ffb_scaling = 1
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use simrig_model::{ComponentKind, CurvePoint, ParamType, Value};
use toml::{Table, Value as Toml};

use crate::error::HostError;

pub type Values = BTreeMap<String, Toml>;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Preset {
    pub name: String,
    /// By component type: applies to every component of that type.
    pub kinds: BTreeMap<ComponentKind, Values>,
    /// By component address: applies to that component only, over `kinds`.
    pub components: BTreeMap<String, Values>,
    /// Custom parameters, by plugin.
    pub custom: BTreeMap<String, Values>,
}

/// What happened to one value of a preset.
#[derive(Clone, Debug, PartialEq)]
pub enum ApplyOutcome {
    /// Set and saved into the driver: active with the app closed.
    Saved,
    /// Set, but not saved (the value lasts until unplugged, or saving failed: see the note).
    Applied { note: Option<String> },
    /// Not applied, with the reason.
    Ignored(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ApplyEntry {
    /// Component address, or the preset key when nothing in the rig matches.
    pub target: String,
    pub item: String,
    pub outcome: ApplyOutcome,
}

fn invalid(path: &Path, message: impl Into<String>) -> HostError {
    HostError::Invalid {
        path: path.to_path_buf(),
        message: message.into(),
    }
}

fn values(table: &Toml) -> Option<Values> {
    table
        .as_table()
        .map(|t| t.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
}

fn nested(table: &Toml) -> Option<BTreeMap<String, Values>> {
    table
        .as_table()?
        .iter()
        .map(|(k, v)| Some((k.clone(), values(v)?)))
        .collect()
}

impl Preset {
    pub fn parse(text: &str, path: &Path) -> Result<Self, HostError> {
        let table: Table = toml::from_str(text).map_err(|e| invalid(path, e.to_string()))?;
        let mut preset = Preset::default();
        for (key, value) in &table {
            match key.as_str() {
                "name" => {
                    preset.name = value
                        .as_str()
                        .ok_or_else(|| invalid(path, "name must be a string"))?
                        .into()
                }
                "component" => {
                    preset.components =
                        nested(value).ok_or_else(|| invalid(path, "bad [component] tables"))?
                }
                "custom" => {
                    preset.custom =
                        nested(value).ok_or_else(|| invalid(path, "bad [custom] tables"))?
                }
                kind => {
                    let kind = ComponentKind::ALL
                        .into_iter()
                        .find(|k| k.key() == kind)
                        .ok_or_else(|| invalid(path, format!("unknown section '{kind}'")))?;
                    preset.kinds.insert(
                        kind,
                        values(value)
                            .ok_or_else(|| invalid(path, format!("[{key}] must be a table")))?,
                    );
                }
            }
        }
        if preset.name.trim().is_empty() {
            return Err(invalid(path, "a preset needs a name"));
        }
        Ok(preset)
    }

    pub fn to_toml(&self) -> String {
        let mut table = Table::new();
        table.insert("name".into(), Toml::String(self.name.clone()));
        let to_table =
            |v: &Values| Toml::Table(v.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
        for (kind, v) in &self.kinds {
            table.insert(kind.key().into(), to_table(v));
        }
        let group = |m: &BTreeMap<String, Values>| {
            Toml::Table(m.iter().map(|(k, v)| (k.clone(), to_table(v))).collect())
        };
        if !self.components.is_empty() {
            table.insert("component".into(), group(&self.components));
        }
        if !self.custom.is_empty() {
            table.insert("custom".into(), group(&self.custom));
        }
        toml::to_string(&table).unwrap_or_default()
    }

    /// File name for a preset name: lowercase letters, digits and dashes.
    pub fn slug(name: &str) -> String {
        let slug: String = name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        slug.split('-')
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join("-")
    }
}

/// A parameter value as stored in a preset.
pub fn to_toml(value: &Value) -> Option<Toml> {
    Some(match value {
        Value::Bool(v) => Toml::Boolean(*v),
        Value::Int(v) => Toml::Integer(*v),
        Value::Float(v) => Toml::Float(*v),
        Value::Text(v) | Value::Choice(v) => Toml::String(v.clone()),
        Value::Curve(points) => Toml::Array(
            points
                .iter()
                .map(|p| Toml::Array(vec![Toml::Float(p.x), Toml::Float(p.y)]))
                .collect(),
        ),
        _ => return None,
    })
}

/// A preset value converted to the parameter's type, and validated.
pub fn from_toml(value: &Toml, ty: &ParamType) -> Result<Value, String> {
    let number = |v: &Toml| v.as_float().or_else(|| v.as_integer().map(|i| i as f64));
    let converted = match (ty, value) {
        (ParamType::Bool, Toml::Boolean(v)) => Value::Bool(*v),
        (ParamType::Int { .. }, Toml::Integer(v)) => Value::Int(*v),
        (ParamType::Float { .. }, v) if number(v).is_some() => {
            Value::Float(number(v).unwrap_or_default())
        }
        (ParamType::Choice { .. }, Toml::String(v)) => Value::Choice(v.clone()),
        (ParamType::Text { .. }, Toml::String(v)) => Value::Text(v.clone()),
        (ParamType::Curve { .. }, Toml::Array(points)) => {
            let point = |p: &Toml| -> Option<CurvePoint> {
                let pair = p.as_array()?;
                match pair.as_slice() {
                    [x, y] => Some(CurvePoint::new(number(x)?, number(y)?)),
                    _ => None,
                }
            };
            Value::Curve(
                points
                    .iter()
                    .map(point)
                    .collect::<Option<_>>()
                    .ok_or("a curve is a list of [x, y] pairs")?,
            )
        }
        (ty, v) => {
            return Err(format!(
                "{} does not fit a {:?} parameter",
                v.type_str(),
                ty.value_kind()
            ));
        }
    };
    ty.validate(&converted).map_err(|e| e.to_string())?;
    Ok(converted)
}

/// Presets stored as `<dir>/<slug>.toml`.
#[derive(Clone, Debug)]
pub struct PresetStore {
    dir: PathBuf,
}

impl PresetStore {
    pub fn new(config_dir: &Path) -> Self {
        Self {
            dir: config_dir.join("presets"),
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.toml", Preset::slug(name)))
    }

    pub fn list(&self) -> Result<Vec<Preset>, HostError> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Ok(Vec::new());
        };
        let mut presets = Vec::new();
        for path in entries.filter_map(|e| e.ok().map(|e| e.path())) {
            if path.extension().is_some_and(|e| e == "toml") {
                presets.push(Preset::parse(&fs::read_to_string(&path)?, &path)?);
            }
        }
        presets.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(presets)
    }

    pub fn get(&self, name: &str) -> Result<Preset, HostError> {
        let path = self.path(name);
        let text = fs::read_to_string(&path)
            .map_err(|_| HostError::NotFound(format!("no preset '{name}'")))?;
        Preset::parse(&text, &path)
    }

    pub fn save(&self, preset: &Preset) -> Result<PathBuf, HostError> {
        if Preset::slug(&preset.name).is_empty() {
            return Err(HostError::InvalidValue(format!(
                "'{}' is not a usable preset name",
                preset.name
            )));
        }
        fs::create_dir_all(&self.dir)?;
        let path = self.path(&preset.name);
        fs::write(&path, preset.to_toml())?;
        Ok(path)
    }

    pub fn delete(&self, name: &str) -> Result<(), HostError> {
        fs::remove_file(self.path(name))
            .map_err(|_| HostError::NotFound(format!("no preset '{name}'")))
    }

    pub fn export(&self, name: &str, to: &Path) -> Result<(), HostError> {
        fs::write(to, self.get(name)?.to_toml())?;
        Ok(())
    }

    /// Imports a preset file; refuses to replace an existing preset unless `replace`.
    pub fn import(&self, from: &Path, replace: bool) -> Result<Preset, HostError> {
        let preset = Preset::parse(&fs::read_to_string(from)?, from)?;
        if !replace && self.path(&preset.name).exists() {
            return Err(HostError::InvalidValue(format!(
                "a preset named '{}' already exists",
                preset.name
            )));
        }
        self.save(&preset)?;
        Ok(preset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_conversion() {
        let text = r#"
            name = "GT3"
            [wheel_base]
            rotation = 540
            [pedals]
            "brake.curve" = [[0, 0], [0.5, 0.35], [1, 1]]
            [component."moza/usb-346e-0003/pedals"]
            "brake.max_force" = 60
            [custom.fanatec]
            ffb_scaling = 1
        "#;
        let preset = Preset::parse(text, Path::new("gt3.toml")).unwrap();
        assert_eq!(
            Preset::parse(&preset.to_toml(), Path::new("x")).unwrap(),
            preset
        );
        let rotation = &preset.kinds[&ComponentKind::WheelBase]["rotation"];
        let ty = ParamType::Float {
            min: 90.0,
            max: 2520.0,
            step: 10.0,
        };
        assert_eq!(from_toml(rotation, &ty).unwrap(), Value::Float(540.0));
        assert!(from_toml(&Toml::Integer(9000), &ty).is_err());
        let curve = &preset.kinds[&ComponentKind::Pedals]["brake.curve"];
        let value = from_toml(curve, &ParamType::Curve { max_points: 8 }).unwrap();
        assert_eq!(
            from_toml(
                &to_toml(&value).unwrap(),
                &ParamType::Curve { max_points: 8 }
            )
            .unwrap(),
            value
        );
        assert_eq!(Preset::slug("GT3 / Night race!"), "gt3-night-race");
    }

    #[test]
    fn rejects_unknown_sections() {
        assert!(Preset::parse("name = 'x'\n[gearbox]\na = 1", Path::new("x")).is_err());
        assert!(Preset::parse("[wheel_base]\nrotation = 1", Path::new("x")).is_err());
    }
}

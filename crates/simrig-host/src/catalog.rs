//! Device models declared by plugins (`catalog/*.toml`) and their images.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use simrig_model::{Appearance, ComponentKind, RimShape};

use crate::manifest::UsbMatch;

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogEntry {
    /// `vendor/model`, e.g. `fanatec/csl-dd`.
    pub id: String,
    pub vendor: String,
    pub name: String,
    pub kind: ComponentKind,
    #[serde(default)]
    pub usb: Vec<UsbMatch>,
    /// The plugin's own picture of the model, relative to the plugin directory: a single image
    /// (static; a rim turns as a whole), or a folder with the same layers as the core's set and
    /// its own `devices.toml`, animated with the same rules.
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub image_license: Option<String>,
    /// Component types the device provides; `?` marks optional ones the wizard asks about.
    #[serde(default)]
    pub provides: Vec<String>,
    /// Named anchor points (fractions of the image size) to animate the rig view.
    #[serde(default)]
    pub overlay: BTreeMap<String, Vec<f64>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub entry: CatalogEntry,
    pub plugin: String,
    pub plugin_dir: PathBuf,
}

/// Where the image of a component comes from, in order of preference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Image {
    /// A picture the user chose (e.g. a photo of their rig).
    User(PathBuf),
    /// The model's image shipped by the plugin.
    Model(PathBuf),
    /// No picture: the app draws the core's generic drawing of the type
    /// (`assets/devices/devices.toml`), in the rim shape the plugin asked for.
    Generic {
        kind: ComponentKind,
        rim: Option<RimShape>,
    },
}

#[derive(Clone, Debug, Default)]
pub struct Catalog {
    models: Vec<Model>,
}

const IMAGE_EXTENSIONS: [&str; 5] = ["svg", "webp", "png", "jpg", "jpeg"];

impl Catalog {
    /// Loads `catalog/*.toml` of a plugin. Returns the problems found.
    pub(crate) fn load_plugin(&mut self, plugin: &str, dir: &Path) -> Vec<String> {
        let mut problems = Vec::new();
        let Ok(entries) = fs::read_dir(dir.join("catalog")) else {
            return problems;
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "toml"))
            .collect();
        files.sort();
        for file in files {
            let parsed = fs::read_to_string(&file)
                .map_err(|e| e.to_string())
                .and_then(|t| toml::from_str::<CatalogEntry>(&t).map_err(|e| e.to_string()));
            match parsed {
                Ok(entry) if self.model(&entry.id).is_some() => {
                    problems.push(format!(
                        "catalog {}: model '{}' already declared",
                        file.display(),
                        entry.id
                    ));
                }
                Ok(entry) => self.models.push(Model {
                    entry,
                    plugin: plugin.into(),
                    plugin_dir: dir.into(),
                }),
                Err(e) => problems.push(format!("catalog {}: {e}", file.display())),
            }
        }
        problems
    }

    pub fn models(&self) -> &[Model] {
        &self.models
    }

    pub fn model(&self, id: &str) -> Option<&Model> {
        self.models.iter().find(|m| m.entry.id == id)
    }

    pub fn for_usb(&self, vid: u16, pid: u16) -> Vec<&Model> {
        self.models
            .iter()
            .filter(|m| m.entry.usb.iter().any(|u| u.matches(vid, pid)))
            .collect()
    }

    /// Image for a component: the user's picture for the model, then the model's image, then
    /// the core's generic drawing of `kind` (with the plugin's appearance hints).
    pub fn image(
        &self,
        model: Option<&str>,
        kind: ComponentKind,
        appearance: &Appearance,
        user_images: Option<&Path>,
    ) -> Image {
        if let (Some(model), Some(dir)) = (model, user_images) {
            let stem = model.replace('/', "_");
            for ext in IMAGE_EXTENSIONS {
                let path = dir.join(format!("{stem}.{ext}"));
                if path.is_file() {
                    return Image::User(path);
                }
            }
        }
        let shipped = model
            .and_then(|m| self.model(m))
            .and_then(|m| m.entry.image.as_ref().map(|i| m.plugin_dir.join(i)))
            .filter(|p| p.exists());
        match shipped {
            Some(path) => Image::Model(path),
            None => Image::Generic {
                kind,
                rim: appearance
                    .rim
                    .filter(|_| kind == ComponentKind::SteeringWheel),
            },
        }
    }
}

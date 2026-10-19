//! Steam Input, as Steam's own files set it: on for generic controllers, or forced on for a
//! game. Steam may then hide a device from games and offer a virtual controller instead, but
//! what it hides is decided while a game runs: these settings only say it may.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Steam Input settings that may take a device from games.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SteamInput {
    /// "Generic controllers" support is on in Steam's settings.
    pub generic: bool,
    /// Installed games with Steam Input forced on: app id and name.
    pub forced: Vec<(String, String)>,
}

/// Where Steam keeps its files, below a home directory (symbolic links resolved, each once).
fn roots(home: &Path) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = [
        ".steam/steam",
        ".steam/root",
        ".local/share/Steam",
        ".steam/debian-installation",
        ".var/app/com.valvesoftware.Steam/.local/share/Steam",
        "snap/steam/common/.local/share/Steam",
    ]
    .iter()
    .filter_map(|p| fs::canonicalize(home.join(p)).ok())
    .filter(|p| p.join("config").is_dir())
    .collect();
    roots.sort();
    roots.dedup();
    roots
}

pub(crate) fn read(home: &Path) -> SteamInput {
    let mut found = SteamInput::default();
    for root in roots(home) {
        let config = parse_file(&root.join("config/config.vdf"));
        found.generic |= config
            .get_path(&[
                "InstallConfigStore",
                "Software",
                "Valve",
                "Steam",
                "SteamController_GenericGamepadSupport",
            ])
            .and_then(Vdf::text)
            == Some("1");
        let names = game_names(&root);
        let Ok(users) = fs::read_dir(root.join("userdata")) else {
            continue;
        };
        for user in users.flatten() {
            let local = parse_file(&user.path().join("config/localconfig.vdf"));
            let Some(Vdf::Map(apps)) = local.get_path(&["UserLocalConfigStore", "apps"]) else {
                continue;
            };
            for (app, settings) in apps {
                // "2" is "Enable Steam Input" in a game's properties ("0" off, "1" default).
                if settings.get("UseSteamControllerConfig").and_then(Vdf::text) != Some("2") {
                    continue;
                }
                // Only installed games; shortcuts to other games have negative ids.
                if let Some(name) = names.get(app)
                    && !found.forced.iter().any(|(a, _)| a == app)
                {
                    found.forced.push((app.clone(), name.clone()));
                }
            }
        }
    }
    found.forced.sort();
    found
}

/// The names of installed games, by app id, from the manifests of every Steam library.
fn game_names(root: &Path) -> BTreeMap<String, String> {
    let mut libraries = vec![root.to_path_buf()];
    for file in ["config/libraryfolders.vdf", "steamapps/libraryfolders.vdf"] {
        if let Some(Vdf::Map(folders)) = parse_file(&root.join(file)).get("libraryfolders") {
            libraries.extend(
                folders
                    .values()
                    .filter_map(|f| f.get("path").and_then(Vdf::text))
                    .map(PathBuf::from),
            );
        }
    }
    libraries.sort();
    libraries.dedup();
    let mut names = BTreeMap::new();
    for library in libraries {
        let Ok(entries) = fs::read_dir(library.join("steamapps")) else {
            continue;
        };
        for entry in entries.flatten() {
            let file = entry.file_name().to_string_lossy().into_owned();
            let Some(app) = file
                .strip_prefix("appmanifest_")
                .and_then(|f| f.strip_suffix(".acf"))
            else {
                continue;
            };
            if let Some(name) = parse_file(&entry.path())
                .get_path(&["AppState", "name"])
                .and_then(Vdf::text)
            {
                names.insert(app.to_string(), name.to_string());
            }
        }
    }
    names
}

/// Valve's text KeyValues: quoted keys with a quoted value or a `{ }` block. Keys are
/// compared without case, as Steam does.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Vdf {
    Text(String),
    Map(BTreeMap<String, Vdf>),
}

impl Vdf {
    fn text(&self) -> Option<&str> {
        match self {
            Vdf::Text(t) => Some(t),
            Vdf::Map(_) => None,
        }
    }

    fn get(&self, key: &str) -> Option<&Vdf> {
        match self {
            Vdf::Map(m) => m
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v),
            Vdf::Text(_) => None,
        }
    }

    fn get_path(&self, path: &[&str]) -> Option<&Vdf> {
        path.iter().try_fold(self, |node, key| node.get(key))
    }
}

fn parse_file(path: &Path) -> Vdf {
    fs::read_to_string(path)
        .map(|t| parse(&t))
        .unwrap_or(Vdf::Map(BTreeMap::new()))
}

fn parse(text: &str) -> Vdf {
    let tokens = tokens(text);
    let mut i = 0;
    Vdf::Map(block(&tokens, &mut i))
}

#[derive(Debug, PartialEq)]
enum Token {
    Str(String),
    Open,
    Close,
}

fn tokens(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => out.push(Token::Open),
            '}' => out.push(Token::Close),
            '"' => {
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => {
                            if let Some(next) = chars.next() {
                                s.push(match next {
                                    'n' => '\n',
                                    't' => '\t',
                                    other => other,
                                });
                            }
                        }
                        _ => s.push(c),
                    }
                }
                out.push(Token::Str(s));
            }
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            c if c.is_whitespace() => {}
            // Unquoted words, and conditions like `[$WIN32]`, are not in the files read here.
            _ => {}
        }
    }
    out
}

fn block(tokens: &[Token], i: &mut usize) -> BTreeMap<String, Vdf> {
    let mut map = BTreeMap::new();
    while *i < tokens.len() {
        match &tokens[*i] {
            Token::Close => {
                *i += 1;
                break;
            }
            Token::Open => *i += 1,
            Token::Str(key) => {
                *i += 1;
                match tokens.get(*i) {
                    Some(Token::Str(value)) => {
                        *i += 1;
                        map.insert(key.clone(), Vdf::Text(value.clone()));
                    }
                    Some(Token::Open) => {
                        *i += 1;
                        let inner = block(tokens, i);
                        map.insert(key.clone(), Vdf::Map(inner));
                    }
                    _ => {}
                }
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_keyvalues() {
        let v = parse("\"A\"\n{\n\t\"b\"\t\t\"1\" // note\n\t\"C\" { \"d\" \"x \\\"y\\\"\" }\n}\n");
        assert_eq!(v.get_path(&["a", "B"]).and_then(Vdf::text), Some("1"));
        assert_eq!(
            v.get_path(&["A", "c", "d"]).and_then(Vdf::text),
            Some("x \"y\"")
        );
    }

    #[test]
    fn steam_input_settings() {
        let home = std::env::temp_dir().join(format!("simrig-steam-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        let steam = home.join(".local/share/Steam");
        let library = home.join("games");
        fs::create_dir_all(steam.join("config")).unwrap();
        fs::create_dir_all(steam.join("userdata/42/config")).unwrap();
        fs::create_dir_all(library.join("steamapps")).unwrap();
        fs::create_dir_all(home.join(".steam")).unwrap();
        std::os::unix::fs::symlink(&steam, home.join(".steam/steam")).unwrap();
        fs::write(
            steam.join("config/libraryfolders.vdf"),
            format!(
                "\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} \"1\" {{ \"path\" \"{}\" }} }}",
                steam.display(),
                library.display()
            ),
        )
        .unwrap();
        fs::write(
            library.join("steamapps/appmanifest_805550.acf"),
            "\"AppState\" { \"appid\" \"805550\" \"name\" \"Assetto Corsa Competizione\" }",
        )
        .unwrap();
        fs::write(
            library.join("steamapps/appmanifest_2399420.acf"),
            "\"AppState\" { \"name\" \"Le Mans Ultimate\" }",
        )
        .unwrap();
        fs::write(
            steam.join("userdata/42/config/localconfig.vdf"),
            "\"UserLocalConfigStore\" { \"apps\" { \
             \"805550\" { \"UseSteamControllerConfig\" \"2\" } \
             \"2399420\" { \"UseSteamControllerConfig\" \"0\" } \
             \"-109\" { \"UseSteamControllerConfig\" \"2\" } } }",
        )
        .unwrap();
        assert_eq!(
            read(&home),
            SteamInput {
                generic: false,
                forced: vec![("805550".into(), "Assetto Corsa Competizione".into())],
            }
        );
        fs::write(
            steam.join("config/config.vdf"),
            "\"InstallConfigStore\" { \"Software\" { \"valve\" { \"Steam\" { \
             \"SteamController_GenericGamepadSupport\" \"1\" } } } }",
        )
        .unwrap();
        assert!(read(&home).generic);
        fs::remove_dir_all(&home).unwrap();
    }
}

use std::env::var_os;
use std::ffi::{OsStr};
use std::fmt::Display;
use std::fs;
use std::io::stdin;
use std::path::{Path, PathBuf};
use std::process::{exit, Command};
use std::time::{SystemTime, UNIX_EPOCH};
use reqwest::blocking::Client;
use reqwest::header::CONTENT_TYPE;

fn main() {
    println!(r#"  _   _            _         _   _                "#);
    println!(r#" | | | | __ _  ___| | ____ _| |_(_)_ __ ___   ___ "#);
    println!(r#" | |_| |/ _` |/ __| |/ / _` | __| | '_ ` _ \ / _ \"#);
    println!(r#" |  _  | (_| | (__|   < (_| | |_| | | | | | |  __/"#);
    println!(r#" |_| |_|\__,_|\___|_|\_\__,_|\__|_|_| |_| |_|\___|"#);

    if run(["git", "-v"]).is_some() {
        ok("Git is installed");
    } else {
        warn("Git is not installed, this is not required for Hackatime but need it to upload your code, download it at https://git-scm.com/downloads")
    }

    check_config();
    check_vscode();
    ok("Everything looks good!");
}

fn check_config() {
    let wakatime_home = variable("WAKATIME_HOME")
        .or_else(|| variable(if cfg!(windows){"USERPROFILE"}else{"HOME"}))
        .expect("No home or WAKATIME_HOME directory found");

    let wakatime_config = Path::new(&wakatime_home).join(".wakatime.cfg");

    if wakatime_config.exists() {
        ok("Found wakatime config file in ".to_string() + &*wakatime_home);
        let mut lines: Vec<String> = fs::read_to_string(&wakatime_config).expect("Could not read wakatime config file").lines().map(str::to_string).collect();

        let mut dirty = false;
        let mut has_url = false;
        let mut has_key = false;
        let mut current_section = String::new();
        for (i, line) in lines.iter_mut().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                current_section = trimmed[1..trimmed.len() - 1].to_lowercase();
            } else if current_section == "settings" {
                let parts: Vec<&str> = line.split('=').collect();
                if parts.len() != 2 {
                    err(format!("Line {i} \"{line}\" in .wakatime.cfg is invalid"));
                    if !ask("Replace full file? (Y/n)").contains("n") {
                        create_config(&wakatime_config);
                        return;
                    } else {
                        exit(1);
                    }
                }
                let val = parts[1].trim().replace("\0", "");
                match parts[0].trim() {
                    "api_url" => {
                        has_url = true;
                        if val != "https://hackatime.hackclub.com/api/hackatime/v1" {
                            warn(format!("Incorrect api url found {val}, replacing..."));
                            *line = "api_url = https://hackatime.hackclub.com/api/hackatime/v1".to_string();
                            dirty = true
                        }
                    }
                    "api_key" => {
                        has_key = true;
                        let mut api_key = val.to_string();
                        validate_key(&mut api_key);
                        if api_key != val {
                            *line = format!("api_key = {api_key}");
                            dirty = true;
                        }
                    }
                    _ => {}
                };
            }
        }
        if !has_url {
            warn("No api url found, adding...");
            lines.push("api_url = https://hackatime.hackclub.com/api/hackatime/v1".to_string());
            dirty = true
        }
        if !has_key {
            warn("No api key found, adding...");
            let mut api_key = variable("HACKATIME_API_KEY").unwrap_or(String::new());
            if api_key.is_empty() {
                api_key = ask_key();
            } else {
                validate_key(&mut api_key);
            }
            lines.push(format!("api_key = {api_key}"));
            dirty = true
        }
        if dirty {
            fs::write(&wakatime_config, lines.join("\n").as_bytes()).expect("Could not write wakatime config file");
            ok("Wakatime config file modified successfully");
        } else {
            ok("Wakatime config looks valid");
        }
    } else {
        warn("No wakatime config file found, creating...");
        create_config(&wakatime_config);
    }
}

fn create_config(wakatime_config: &PathBuf) {
    let mut api_key = variable("HACKATIME_API_KEY").unwrap_or(String::new());
    if api_key.is_empty() {
        api_key = ask_key()
    } else {
        validate_key(&mut api_key);
    }

    fs::write(wakatime_config, ("[settings]
api_url = https://hackatime.hackclub.com/api/hackatime/v1
api_key = ".to_owned() +api_key.as_str()+"
heartbeat_rate_limit_seconds = 30").as_bytes()).expect("Could not write wakatime config file");

    ok("Wakatime config file created successfully");
}

fn ask_key() -> String {
    let mut api_key = ask("What is your API key?");//TODO link
    validate_key(&mut api_key);
    api_key
}

fn validate_key(api_key: &mut String) {
    let code = Client::new()
        .post("https://hackatime.hackclub.com/api/hackatime/v1/users/current/heartbeats")
        .bearer_auth(&api_key)
        .header(CONTENT_TYPE, "application/json")
        .body(r#"[{"type":"file","time":"#.to_string()
            + &*SystemTime::now().duration_since(UNIX_EPOCH).expect("Hackatime does not exist yet :(").as_secs().to_string()
            + r#","entity":"test.txt","language":"Text"}]"#)
        .send().expect("Failed to send heartbeat").status();
    if code == 401 {
        err(format!("Invalid API key {api_key}"));
        *api_key = ask_key();
    }
    if !code.is_success() {
        panic!("Failed to send heartbeat!");
    } else {
        ok("Successfully sent heartbeat");
    }
}

fn check_vscode() {
    if let Some(output) = run(["code", "--list-extensions"]) {
        if output.contains("wakatime.vscode-wakatime") {
            ok("Wakatime is installed in VS Code");
            let path;

            if cfg!(target_os = "windows") {
                path = variable("APPDATA")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from(variable("USERPROFILE").expect("No home or APPDATA directory found")).join("AppData/Roaming"))
                    .join("Code/User/settings.json");
            } else if cfg!(target_os = "macos") {
                path = PathBuf::from(variable("HOME").expect("No home directory found")).join("Library/Application Support/Code/User/settings.json");
            } else {
                path = variable("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(variable("HOME").expect("No home directory found"))
                    .join(".config")).join("Code/User/settings.json");
            }
            if let Ok(content) = fs::read_to_string(path) {
                if content.contains("wakatime.") {
                    warn("Your VS Code settings are overriding Hackatime, please remove all settings related to Wakatime");
                } else {
                    ok("VS Code probably does not override your api settings")
                }
            } else {
                warn("Could not find the VS Code settings file, make sure it does not override your Wakatime api settings")
            }
        } else {
            warn("VS Code does not have the Wakatime extension installed");
            if !ask("Install? (Y/n)").contains("n") {
                if run(["code", "--install-extension", "wakatime.vscode-wakatime", "--force"]).is_some() {
                    ok("Successfully installed Wakatime for VS Code");
                } else {
                    err("Failed to install Wakatime for VS Code")
                }
            }
        }
    } else {
        info("VS Code not found");
    }
}

fn err<S: Display>(text: S) {
    eprintln!("\x1B[38;5;196m❌  {text}\x1B[0m");
}

fn warn<S: Display>(text: S) {
    println!("\x1B[38;5;190m⚠️ {text}\x1B[0m");
}

fn info<S: Display>(text: S) {
    println!("\x1B[38;5;27mℹ️ {text}\x1B[0m");
}

fn ok<S: Display>(text: S) {
    println!("\x1B[38;5;40m✔️ {text}\x1B[0m");
}

fn ask<S: Display>(text: S) -> String {
    println!("❓  {text}");
    let mut response =  String::new();
    stdin().read_line(&mut response).expect("Failed to read from stdin");
    response.trim().to_string()
}

fn variable(key: &str) -> Option<String> {
    let val = var_os(key)?.to_str()?.trim().to_string();
    if val.is_empty() || !Path::new(&val).exists() {
        return None;
    }
    Some(val)
}

fn run<I: IntoIterator<Item = S>, S: AsRef<OsStr>,>(args: I) -> Option<String> {
    let mut command;
    if cfg!(target_os = "windows") {
        command = Command::new("cmd");
        command.arg("/C");
    } else {
        command = Command::new(variable("SHELL").unwrap_or("/bin/sh".to_string()));
        command.args(&["-l", "-c"]);
    }
    if let Ok(result) = command.args(args).output() {
        if result.status.success() {
            return Some(String::from_utf8(result.stdout).expect("Stdout returned non-UTF data"));
        }
    }
    None
}
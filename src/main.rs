extern crate core;

use std::env::var_os;
use std::ffi::{OsStr, OsString};
use std::fmt::Display;
use std::fs;
use std::fs::File;
use std::io::{stdin, stdout, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{exit, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};
use reqwest::blocking::{get, Client};
use reqwest::header::CONTENT_TYPE;

fn main() {
    println!(r#"  _   _            _         _   _                "#);
    println!(r#" | | | | __ _  ___| | ____ _| |_(_)_ __ ___   ___ "#);
    println!(r#" | |_| |/ _` |/ __| |/ / _` | __| | '_ ` _ \ / _ \"#);
    println!(r#" |  _  | (_| | (__|   < (_| | |_| | | | | | |  __/"#);
    println!(r#" |_| |_|\__,_|\___|_|\_\__,_|\__|_|_| |_| |_|\___|"#);
    info("Report issues to https://github.com/itsFelix5/hackatime/issues");

    check_git();
    check_config();
    check_vscode();
    check_jetbrains();
    #[cfg(unix)]
    check_terminal();
    ok("Everything looks good! Code for a few minutes and check if it appears on https://hackatime.hackclub.com");
    info("The SOM website has a very long cache and should not be trusted for accurate time data");

    print!("Press enter to close");
    stdout().flush().unwrap();
    stdin().read(&mut [0u8]).unwrap();
} 

fn check_git() {
    if run("git -v").is_some() {
        ok("Git is installed");
    } else {
        warn("Git is not installed, this is not required for Hackatime but it is needed to upload your code");
        if !ask("Install git? (Y/n) ").contains("n") {
            if cfg!(target_os = "windows") {
                if run_with_output("winget install --id Git.Git -e --source winget") {
                    ok("Successfully installed git using winget")
                } else {
                    err("Failed to install git using winget. Download git manually at https://git-scm.com/downloads/win");
                }
            } else if cfg!(target_os = "macos") {
                if run_with_output("brew install git") {
                    ok("Successfully installed git using homebrew")
                } else {
                    err("Failed to install git using homebrew. Download git manually at https://git-scm.com/downloads/mac");
                }
            } else {
                let mut command = "";
                let distro = fs::read_to_string("/etc/os-release").unwrap_or_default();
                if distro.contains("ubuntu") || distro.contains("debian") {
                    run("sudo apt-get update");
                    command = "apt-get install -y git";
                } else if distro.contains("fedora") || distro.contains("rhel") || distro.contains("centos") {
                    command = "dnf install -y git";
                } else if distro.contains("arch") {
                    command = "pacman -Sy git --noconfirm";
                } else if distro.contains("nix") {
                    command = "nix-env -i git";
                } else {
                    err("Unsupported distro, please install Git manually at https://git-scm.com/downloads");
                }
                if !command.is_empty() {
                    if run_with_output(&*("sudo".to_string() + command)) {
                        ok("Successfully installed git")
                    } else {
                        err("Failed to install git, please install Git manually at https://git-scm.com/downloads")
                    }
                }
            }
        } else {
            info("Download git manually at https://git-scm.com/downloads");
        }
    }
}

fn check_config() {
    let wakatime_home = path_from_env("WAKATIME_HOME")
        .or_else(|| path_from_env(if cfg!(windows){"USERPROFILE"}else{"HOME"}))
        .expect("No home or WAKATIME_HOME directory found");

    let wakatime_config = Path::new(&wakatime_home).join(".wakatime.cfg");

    let mut api_key = String::new();
    if wakatime_config.exists() {
        ok("Found wakatime config file in ".to_string() + &wakatime_home.to_string_lossy());
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
                    if !ask("Replace full file? (Y/n) ").contains("n") {
                        api_key = ask_key();

                        fs::write(wakatime_config, ("[settings]
api_url = https://hackatime.hackclub.com/api/hackatime/v1
api_key = ".to_owned() +api_key.as_str()+"
heartbeat_rate_limit_seconds = 30").as_bytes()).expect("Could not write wakatime config file");

                        ok("Wakatime config file created successfully");
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
                            warn(format!("Incorrect api url found {val}"));
                            if !ask("Replace with https://hackatime.hackclub.com/api/hackatime/v1? (Y/n) ").contains("n") {
                                *line = "api_url = https://hackatime.hackclub.com/api/hackatime/v1".to_string();
                                dirty = true
                            }
                        }
                    }
                    "api_key" => {
                        has_key = true;
                        api_key = val.to_string();
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
            api_key = ask_key();
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
        api_key = ask_key();

        fs::write(wakatime_config, ("[settings]
api_url = https://hackatime.hackclub.com/api/hackatime/v1
api_key = ".to_owned() +api_key.as_str()+"
heartbeat_rate_limit_seconds = 30").as_bytes()).expect("Could not write wakatime config file");

        ok("Wakatime config file created successfully");
    }

    if let Ok(res) = get("https://hackatime.hackclub.com/api/hackatime/v1/users/me/statusbar/today?api_key=".to_string() + &*api_key).and_then(|r| r.text()) {
        let time = &res.split("\"total_seconds\":").collect::<Vec<&str>>()[1];
        let time: u16 = time[..time.len() - 3].parse().expect("Invalid number returned from API");
        let minutes = time / 60;
        info(format!("You have coded for {}h {}m today according to Hackatime", minutes / 60, minutes % 60));
    } else {
        err("Could not fetch current hours")
    }
}

fn ask_key() -> String {
    let mut api_key = var_os("HACKATIME_API_KEY").map(|key|key.to_string_lossy().to_string())
        .unwrap_or_else(|| ask("What is your API key? "));
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
        *api_key = ask("What is your API key? ");//TODO link
        validate_key(api_key);
    } else if !code.is_success() {
        panic!("{} Failed to send heartbeat!", code.as_str());
    } else {
        ok("Successfully sent heartbeat");
    }
}

fn check_vscode() {
    if let Some(output) = run("code --list-extensions") {
        if output.contains("wakatime.vscode-wakatime") {
            ok("Wakatime is installed in VS Code");
            let path;

            if cfg!(target_os = "windows") {
                path = path_from_env("APPDATA")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from(path_from_env("USERPROFILE").expect("No home or APPDATA directory found")).join("AppData/Roaming"))
                    .join("Code/User/settings.json");
            } else if cfg!(target_os = "macos") {
                path = PathBuf::from(path_from_env("HOME").expect("No home directory found")).join("Library/Application Support/Code/User/settings.json");
            } else {
                path = path_from_env("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(path_from_env("HOME").expect("No home directory found"))
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
            if !ask("Install? (Y/n) ").contains("n") {
                if run_with_output("code --install-extension wakatime.vscode-wakatime --force") {
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

fn check_jetbrains() {
    let mut path;
    if cfg!(target_os = "windows") {
        path = PathBuf::from(path_from_env("LOCALAPPDATA").expect("No local appdata found"));
    } else if cfg!(target_os = "macos") {
        path = PathBuf::from(path_from_env("HOME").expect("No home directory found")).join("Library/Application Support");
        if !Path::new(&path).exists() {path = PathBuf::from("/usr/local/bin");}
    } else {
        path = PathBuf::from(path_from_env("HOME").expect("No home directory found")).join(".local/share");
    }
    path = path.join("JetBrains/Toolbox/scripts");
    if let Ok(dir) = fs::read_dir(path) { 
        for entry in dir {
            if let Ok(entry) = entry {
                let file = entry.file_name().to_str().expect("Invalid unicode").to_string();
                if let Some(name) = if cfg!(windows){file.strip_suffix(".cmd")}else{if file.ends_with(".cmd"){None}else{Some(&*file)}} {
                    if !ask(format!("Do you want to install Wakatime for {}? (Make sure it is not open!) (Y/n) ", name)).contains("n") {
                        if run_with_output(&*(entry.path().to_string_lossy().to_string() + " installPlugins com.wakatime.intellij.plugin")) {
                            ok("Successfully installed Wakatime for ".to_owned() + name);
                        } else {
                            err("Failed to install Wakatime for".to_owned() + name);
                        }
                    }
                }
            }
        }
    } else {
        info("Jetbrains Toolbox not found, skipping IDEs") 
    }
}

#[cfg(unix)]
fn check_terminal() {
    if run("terminal-wakatime").is_some() {
        ok("Wakatime is installed in the terminal");
        check_terminal_registered(false);
    } else {
        if !ask("Do you want to install Wakatime in the terminal? (Y/n) ").contains("n") {
            if let Ok(Some(res)) = get("https://api.github.com/repos/hackclub/terminal-wakatime/releases/latest")
                .and_then(|r|r.text())
                .map(|r| r.lines().find(|l| l.trim().starts_with("\"tag_name\":"))){
                let os = if cfg!(target_os = "windows") {"windows"} else if cfg!(target_os = "macos") {"darwin"} else {"linux"};
                let arch = if cfg!(any(target_arch = "arm", target_arch = "aarch64")) {"arm64"} else {"amd64"};
                let url = format!("https://github.com/hackclub/terminal-wakatime/releases/download/{}/terminal-wakatime-{os}-{arch}", &res.trim()[12..18]);
                let mut target = PathBuf::from("/usr/local/bin/terminal-wakatime");
                if target.metadata().map(|m| m.permissions().readonly()).unwrap_or(true) {
                    target = path_from_env("HOME").expect("No home directory found").join(".wakatime/terminal-wakatime");
                    fs::create_dir_all(&target).expect("Failed to create ~/.wakatime");
                }

                if let Ok(data) = get(url).and_then(|r| r.bytes()) {
                    if File::create(&target).write_all(data) {
                        ok("Successfully downloaded Wakatime for the terminal to " + target);
                        check_terminal_registered(&target);
                    } else {
                        err("Failed write to " + target);
                    }
                } else { 
                    err("Failed to get the latest binary")
                }
            } else {
                err("Failed to fetch latest tag");
                return;
            }
        }
    }
}

fn check_terminal_registered(add_path: bool) {
    for (name, file) in [("bash", ".bashrc"), ("zsh", ".zshrc"), ("fish", ".config/fish/config.fish")] {
        let path = path_from_env("HOME").expect("No home directory found").join(file);
        if !path.exists() {
            continue;
        }
        if let Ok(mut content) = fs::read_to_string(&path) {
            if content.contains("terminal-wakatime") {
                ok(file.to_string() + " has time tracking setup");
                continue;
            }
            if ask(format!("Do you want to setup time tracking for {name}? (Y/n) ")).contains("n") {
                continue;
            }
            content += "\n";
            if name == "fish" {
                if add_path {
                    content += "set -x PATH \"$HOME/.wakatime\" $PATH";
                }
                content += "terminal-wakatime init fish | source";
            } else {
                if let Some(output) = run("terminal-wakatime init") {
                    if add_path {
                        content += "export PATH=\"$HOME/.wakatime:$PATH\"";
                    }
                    content += &*format!("eval {output}");
                }
            }
            ok("Registered wakatime-terminal for ".to_string() + name);
        }
    }
    info("Restart your terminal for time tracking to work")
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
    print!("❓  {text}");
    let _ = stdout().flush();
    let mut response = String::new();
    stdin().read_line(&mut response).expect("Failed to read from stdin");
    response.trim().to_lowercase()
}

fn path_from_env(key: &str) -> Option<PathBuf> {
    let val = PathBuf::from(var_os(key)?.to_str()?.trim().to_string());
    if !Path::new(&val).exists() {
        return None;
    }
    Some(val)
}

fn run<S: AsRef<OsStr>>(args: S) -> Option<String> {
    let mut command;
    if cfg!(windows) {
        command = Command::new("cmd");
        command.arg("/C");
    } else {
        command = Command::new(path_from_env("SHELL").map(|v| v.into_os_string()).unwrap_or(OsString::from("/bin/sh")));
        command.args(&["-l", "-c"]);
    }
    if let Ok(result) = command.arg(args).output() {
        if result.status.success() {
            return Some(String::from_utf8(result.stdout).expect("Stdout returned non-UTF data"));
        }
    }
    None
}

fn run_with_output(args: &str) -> bool {
    let mut command;
    if cfg!(windows) {
        command = Command::new("cmd");
        command.arg("/C");
    } else {
        command = Command::new(path_from_env("SHELL").map(|v| v.into_os_string()).unwrap_or(OsString::from("/bin/sh")));
        command.args(&["-l", "-c"]);
    }
    command.arg(args).stdout(Stdio::inherit()).stderr(Stdio::inherit()).stdin(Stdio::inherit()).status().is_ok_and(|o| o.success())
}
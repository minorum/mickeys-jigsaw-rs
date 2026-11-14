pub mod app;
pub mod assets;
pub mod audio;
pub mod decompress;
pub mod graphics;
pub mod input;
pub mod puzzle;
pub mod puzzle_render;
mod sdl_platform;
pub mod setup;

use std::env;
use std::path::{Path, PathBuf};

use crate::app::GameApp;

fn find_game_dir() -> PathBuf {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if cwd.join("MJP.NFT").exists() {
        return cwd;
    }
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    if exe_dir.join("MJP.NFT").exists() {
        return exe_dir;
    }
    cwd
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let start_in_setup = args
        .iter()
        .any(|a| a.eq_ignore_ascii_case("/S") || a.eq_ignore_ascii_case("--setup"));

    let game_dir = find_game_dir();
    let app = match GameApp::new(game_dir, start_in_setup) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Failed to initialize Mickey's Jigsaw Puzzles: {}", e);
            return;
        }
    };

    if let Err(e) = sdl_platform::run(app) {
        eprintln!("SDL3 runtime error: {}", e);
    }
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;

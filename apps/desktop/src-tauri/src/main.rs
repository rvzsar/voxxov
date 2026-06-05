// Скрываем консоль в release-сборке под Windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    gigaam_desktop_lib::run();
}

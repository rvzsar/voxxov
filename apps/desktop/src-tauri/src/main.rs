// Скрываем консоль в release-сборке под Windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// mimalloc вместо системного аллокатора: дешевле на аллокационно-нагруженных
// путях (токены ASR, парсинг stderr yt-dlp). Одна строка, без рисков.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    voxxov_lib::run();
}

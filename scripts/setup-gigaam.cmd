@echo off
rem Установка Python-зависимостей для ASR (GigaAM) в виртуальное окружение.
rem Использовать если хотите встроить gigaam вместо внешнего CLI.
setlocal
python -m venv .venv
call .venv\Scripts\activate
pip install --upgrade pip
pip install gigaam onnxruntime soundfile
echo ✔ gigaam installed. Запустите gigaam --help для проверки.

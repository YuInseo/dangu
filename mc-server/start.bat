@echo off
rem dangu 서버 켜기 (윈도우). start.bat master = 담비 마스터.
rem 꺼지면(충돌) 5초 뒤 다시 켭니다. stop 으로 끄면 그대로 끝납니다.
chcp 65001 >nul
cd /d "%~dp0"
where cargo >nul 2>nul
if errorlevel 1 (
  echo Rust가 없습니다. https://rustup.rs 에서 설치한 뒤 다시 실행하세요.
  pause
  exit /b 1
)
echo 빌드하는 중... (처음에는 몇 분 걸립니다)
cargo build --release
if errorlevel 1 (
  pause
  exit /b 1
)
:loop
target\release\mc-server.exe %*
if %errorlevel%==0 goto end
echo 서버가 비정상 종료했습니다. 5초 뒤 다시 켭니다 - 멈추려면 Ctrl+C
timeout /t 5 >nul
goto loop
:end
echo 서버를 껐습니다.
pause

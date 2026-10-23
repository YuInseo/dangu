#!/usr/bin/env sh
# 윈도우용 받아서 바로 켜는 묶음 (dist/dangu-server-windows.zip).
# 필요: rustup target add x86_64-pc-windows-gnu, gcc-mingw-w64-x86-64, zip
set -e
cd "$(dirname "$0")/.."
CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
  cargo build --release --target x86_64-pc-windows-gnu
out=dist/dangu-server
rm -rf "$out" dist/dangu-server-windows.zip
mkdir -p "$out"
cp target/x86_64-pc-windows-gnu/release/mc-server.exe "$out/"
printf '%s\r\n' \
  '@echo off' \
  'rem dangu 서버 켜기. 꺼지면(충돌) 5초 뒤 다시 켭니다. stop 으로 끄면 끝납니다.' \
  'chcp 65001 >nul' \
  'cd /d "%~dp0"' \
  ':loop' \
  'mc-server.exe %*' \
  'if %errorlevel%==0 goto end' \
  'echo 서버가 비정상 종료했습니다. 5초 뒤 다시 켭니다 - 멈추려면 Ctrl+C' \
  'timeout /t 5 >nul' \
  'goto loop' \
  ':end' \
  'echo 서버를 껐습니다.' \
  'pause' > "$out/서버 켜기.bat"
printf '%s\r\n' \
  'dangu 서버 (Minecraft Java 26.3)' \
  '' \
  '1. "서버 켜기.bat" 더블클릭 (윈도우 방화벽이 물으면 허용)' \
  '2. 처음 켜면 server.env 가 생깁니다. 메모장으로 열어' \
  '   MC_OPS=내마크닉네임  을 넣고, 정품 계정이 없으면 MC_ONLINE_MODE=false' \
  '   로 바꾼 뒤 창을 닫고 다시 켜세요.' \
  '3. 마인크래프트 26.3 → 멀티플레이 → 서버 추가 → 주소 localhost' \
  '4. 웹 관리 화면 주소(http://127.0.0.1:25580/#token=...)는 서버 창에 찍힙니다.' \
  '5. 친구가 들어오려면 이 컴퓨터의 IP, 공유기에서 25565·25566 포트 포워딩.' \
  '' \
  '끄기: 서버 창에 stop' > "$out/읽어 주세요.txt"
(cd dist && zip -qr dangu-server-windows.zip dangu-server)
ls -la dist/dangu-server-windows.zip

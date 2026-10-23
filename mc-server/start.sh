#!/usr/bin/env sh
# dangu 서버 켜기 (리눅스·맥).
#   ./start.sh          혼자 도는 서버
#   ./start.sh master   담비 마스터 (인스턴스마다 프로세스)
# 꺼지면(충돌) 5초 뒤 다시 켭니다. stop 으로 끄면 그대로 끝납니다.
cd "$(dirname "$0")" || exit 1

if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust가 없습니다. https://rustup.rs 에서 설치한 뒤 다시 실행하세요."
  exit 1
fi

echo "빌드하는 중… (처음에는 몇 분 걸립니다)"
cargo build --release || exit 1

# 서버가 "다시 켜 달라"(업데이트, 코드 75)로 끝나면 곧바로 다시 켭니다.
export DANGU_LAUNCHER=start.sh
while true; do
  ./target/release/mc-server "$@"
  code=$?
  if [ "$code" -eq 0 ]; then
    echo "서버를 껐습니다."
    break
  fi
  if [ "$code" -eq 75 ]; then
    echo "업데이트한 버전으로 다시 켭니다…"
    continue
  fi
  echo "서버가 비정상 종료했습니다 (코드 $code). 5초 뒤 다시 켭니다 — 멈추려면 Ctrl+C"
  sleep 5
done

#!/usr/bin/env bash
# 에뮬레이터에 Lumen을 깔고 켠 뒤, 시간대별 화면·UI 트리·로그를 모은다.
set -x
OUT=e2e
mkdir -p "$OUT"
PKG=com.dangu.lumen

adb wait-for-device
adb shell settings put global window_animation_scale 0
adb shell settings put global transition_animation_scale 0
adb shell settings put global animator_duration_scale 0
adb shell dumpsys package com.google.android.webview | grep -m1 versionName > "$OUT/webview-version.txt" || true

adb install -r lumen.apk
adb shell pm grant $PKG android.permission.POST_NOTIFICATIONS || true
adb logcat -c
adb shell am start -W -n $PKG/.MainActivity

for t in 8 20 35; do
  sleep $(( t - ${prev:-0} )); prev=$t
  adb exec-out screencap -p > "$OUT/shot-${t}s.png"
done

adb shell uiautomator dump /sdcard/ui.xml && adb pull /sdcard/ui.xml "$OUT/ui.xml"

# 화면 위 덧층(Compose)이 터치를 막지 않는지: 첫 입력칸을 눌러 글자를 넣어 본다.
XY=$(python3 - "$OUT/ui.xml" <<'PY'
import re, sys
xml = open(sys.argv[1], encoding="utf-8").read()
m = re.search(r'class="android.widget.EditText"[^>]*bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"', xml)
if m:
    x1, y1, x2, y2 = map(int, m.groups())
    print((x1 + x2) // 2, (y1 + y2) // 2)
PY
)
echo "입력칸: ${XY:-없음}" > "$OUT/input-test.txt"
if [ -n "$XY" ]; then
  adb shell input tap $XY
  sleep 2
  adb shell input text "lumen-e2e"
  sleep 2
  adb exec-out screencap -p > "$OUT/shot-typed.png"
  adb shell uiautomator dump /sdcard/ui2.xml && adb pull /sdcard/ui2.xml "$OUT/ui-typed.xml"
  grep -c "lumen-e2e" "$OUT/ui-typed.xml" >> "$OUT/input-test.txt" || echo "0 (글자가 안 들어감)" >> "$OUT/input-test.txt"
fi

# 떠 있는 단추: 누르면 클래식 서랍, 길게 누르면 설정
BXY=$(python3 - "$OUT/ui.xml" <<'PY'
import re, sys
xml = open(sys.argv[1], encoding="utf-8").read()
m = re.search(r'content-desc="Lumen 설정"[^>]*bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"', xml)
if m:
    x1, y1, x2, y2 = map(int, m.groups())
    print((x1 + x2) // 2, (y1 + y2) // 2)
PY
)
adb shell input keyevent 111   # 키보드 닫기
sleep 1
if [ -n "$BXY" ]; then
  adb shell input tap $BXY
  sleep 2
  adb exec-out screencap -p > "$OUT/shot-drawer.png"
  adb shell input keyevent 4
  sleep 1
  adb shell input swipe $BXY $BXY 900
  sleep 2
  adb exec-out screencap -p > "$OUT/shot-settings.png"
  adb shell input keyevent 4
  sleep 1
fi
# 왼쪽 가장자리에서 밀어 서랍 열기
H=$(adb shell wm size | grep -oE '[0-9]+x[0-9]+' | tail -1)
W=${H%x*}; HH=${H#*x}
adb shell input swipe 3 $((HH / 2)) $((W * 3 / 4)) $((HH / 2)) 300
sleep 2
adb exec-out screencap -p > "$OUT/shot-swipe.png"
adb shell input keyevent 4
sleep 5
adb exec-out screencap -p > "$OUT/shot-end.png"
adb logcat -d > "$OUT/logcat-full.txt"
grep -E "chromium|Lumen|AndroidRuntime|WebView|cr_|CONSOLE|lumen" "$OUT/logcat-full.txt" | tail -400 > "$OUT/logcat.txt"
adb shell pidof $PKG > "$OUT/pid.txt" || echo "앱 프로세스 없음(죽음)" > "$OUT/pid.txt"
# ── 2부: 가짜 디스코드(E2E 빌드에만 들어 있음)로 네이티브 화면을 끝까지 눌러 본다 ──
find_xy() {  # $1=ui.xml $2=속성 이름 $3=값 → "x y"
  python3 - "$1" "$2" "$3" <<'PY'
import re, sys
xml = open(sys.argv[1], encoding="utf-8").read()
pat = r'%s="%s"[^>]*bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"' % (re.escape(sys.argv[2]), re.escape(sys.argv[3]))
m = re.search(pat, xml)
if m:
    x1, y1, x2, y2 = map(int, m.groups())
    print((x1 + x2) // 2, (y1 + y2) // 2)
PY
}
dump() { adb shell uiautomator dump /sdcard/u.xml >/dev/null && adb pull /sdcard/u.xml "$OUT/$1" >/dev/null; }

adb shell am force-stop $PKG
adb shell am start -W -n $PKG/.MainActivity --ez lumen_mock true
sleep 10
adb exec-out screencap -p > "$OUT/mock-1-chat.png"
dump mock-1.xml

XY=$(find_xy "$OUT/mock-1.xml" content-desc "서랍")
echo "서랍 단추: ${XY:-없음}" > "$OUT/mock-result.txt"
if [ -n "$XY" ]; then
  adb shell input tap $XY; sleep 2
  adb exec-out screencap -p > "$OUT/mock-2-drawer.png"
  dump mock-2.xml
  XY=$(find_xy "$OUT/mock-2.xml" text "공지")
  echo "공지 채널: ${XY:-없음}" >> "$OUT/mock-result.txt"
  if [ -n "$XY" ]; then adb shell input tap $XY; sleep 3; fi
  adb exec-out screencap -p > "$OUT/mock-3-notice.png"
  dump mock-3.xml
  grep -q '# 공지' "$OUT/mock-3.xml" && echo "채널 이동: 됨" >> "$OUT/mock-result.txt" || echo "채널 이동: 안 됨" >> "$OUT/mock-result.txt"
  # 다시 일반으로
  XY=$(find_xy "$OUT/mock-3.xml" content-desc "서랍"); [ -n "$XY" ] && adb shell input tap $XY && sleep 2
  dump mock-4.xml
  XY=$(find_xy "$OUT/mock-4.xml" text "일반"); [ -n "$XY" ] && adb shell input tap $XY && sleep 3
fi

dump mock-5.xml
XY=$(python3 - "$OUT/mock-5.xml" <<'PY'
import re, sys
xml = open(sys.argv[1], encoding="utf-8").read()
m = re.search(r'class="android.widget.EditText"[^>]*bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"', xml)
if m:
    x1, y1, x2, y2 = map(int, m.groups()); print((x1 + x2) // 2, (y1 + y2) // 2)
PY
)
echo "입력칸: ${XY:-없음}" >> "$OUT/mock-result.txt"
if [ -n "$XY" ]; then
  adb shell input tap $XY; sleep 1
  adb shell input text "hello-lumen"; sleep 1
  dump mock-6.xml
  XY=$(find_xy "$OUT/mock-6.xml" content-desc "보내기")
  echo "보내기 단추: ${XY:-없음}" >> "$OUT/mock-result.txt"
  [ -n "$XY" ] && adb shell input tap $XY && sleep 3
  adb shell input keyevent 111; sleep 1
  adb exec-out screencap -p > "$OUT/mock-4-sent.png"
  dump mock-7.xml
  echo "보낸 메시지 화면에 보임: $(grep -o 'hello-lumen' "$OUT/mock-7.xml" | wc -l)회" >> "$OUT/mock-result.txt"
fi
adb shell pidof $PKG >> "$OUT/mock-result.txt" || echo "앱 죽음" >> "$OUT/mock-result.txt"
adb logcat -d | grep -E "AndroidRuntime|FATAL" | tail -40 > "$OUT/crash.txt"

ls -la "$OUT"
exit 0

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
ls -la "$OUT"
exit 0

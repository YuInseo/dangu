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

for t in 8 20 40 70; do
  sleep $(( t - ${prev:-0} )); prev=$t
  adb exec-out screencap -p > "$OUT/shot-${t}s.png"
done

adb shell uiautomator dump /sdcard/ui.xml && adb pull /sdcard/ui.xml "$OUT/ui.xml"
adb logcat -d > "$OUT/logcat-full.txt"
grep -E "chromium|Lumen|AndroidRuntime|WebView|cr_|CONSOLE|lumen" "$OUT/logcat-full.txt" | tail -400 > "$OUT/logcat.txt"
adb shell pidof $PKG > "$OUT/pid.txt" || echo "앱 프로세스 없음(죽음)" > "$OUT/pid.txt"
ls -la "$OUT"
exit 0

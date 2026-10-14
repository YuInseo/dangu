#!/usr/bin/env bash
# 에뮬레이터에 소리를 깔고, 시험용 음악을 넣고, 화면을 찍는다.
set -x
mkdir -p shots
adb install -r Sori.apk
adb shell mkdir -p /sdcard/Music
adb push "music/Sori Samples" /sdcard/Music/
adb shell pm grant app.sori.player android.permission.READ_MEDIA_AUDIO || true
adb shell pm grant app.sori.player android.permission.READ_MEDIA_IMAGES || true
adb shell pm grant app.sori.player android.permission.READ_EXTERNAL_STORAGE || true
# 미디어 스캐너가 새 파일을 알게 한다(앱은 파일을 직접 읽지만 권한 확인에 쓰인다).
adb shell am broadcast -a android.intent.action.MEDIA_SCANNER_SCAN_FILE -d "file:///sdcard/Music" || true
adb shell monkey -p app.sori.player -c android.intent.category.LAUNCHER 1
sleep 30
adb forward tcp:4380 tcp:4380 || true
curl -s --max-time 10 http://127.0.0.1:4380/api/library || echo "서버에 닿지 않음"
echo
adb exec-out screencap -p > shots/android-1-home.png
size=$(adb shell wm size | tr -dc '0-9x\n' | tail -1)
w=${size%x*}; h=${size#*x}
echo "screen $w x $h"
# 아래쪽 탭 막대: 홈, 앨범, 아티스트, 라이브, 검색
y=$((h - h * 9 / 100))
tap() { adb shell input tap "$1" "$y"; sleep 6; }
tap $((w * 3 / 10)); adb exec-out screencap -p > shots/android-2-albums.png
# 첫 앨범을 연다
adb shell input tap $((w / 4)) $((h * 40 / 100)); sleep 6; adb exec-out screencap -p > shots/android-3-album.png
# 재생
adb shell input tap $((w * 18 / 100)) $((h * 70 / 100)); sleep 6; adb exec-out screencap -p > shots/android-4-playing.png
tap $((w / 2)); adb exec-out screencap -p > shots/android-5-artists.png
adb logcat -d | grep -iE "sori|tauri|rust|chromium" | tail -150 > shots/logcat.txt || true
ls -la shots

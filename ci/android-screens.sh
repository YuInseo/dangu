#!/usr/bin/env bash
# 에뮬레이터에 소리를 깔고, 시험용 음악을 넣고, 여러 화면을 찍는다.
# 누를 곳은 좌표가 아니라 글자로 찾는다(uiautomator가 웹뷰 안의 글자와 aria-label도 보여 준다).
set -x
mkdir -p shots
adb install -r Sori.apk
adb shell mkdir -p /sdcard/Music
adb push "music/Sori Samples" /sdcard/Music/
adb shell pm grant app.sori.player android.permission.READ_MEDIA_AUDIO || true
adb shell pm grant app.sori.player android.permission.READ_MEDIA_IMAGES || true
adb shell pm grant app.sori.player android.permission.READ_EXTERNAL_STORAGE || true
adb shell monkey -p app.sori.player -c android.intent.category.LAUNCHER 1
sleep 30
adb forward tcp:4380 tcp:4380 || true
curl -s --max-time 10 http://127.0.0.1:4380/api/library || echo "서버에 닿지 않음"
echo

n=0
shot() {
  n=$((n + 1))
  adb exec-out screencap -p > "shots/android-$(printf %02d $n)-$1.png"
}

# 화면에서 글자(또는 aria-label)가 $1인 것의 가운데를 누른다. 없으면 1을 돌려준다.
tap_label() {
  adb shell uiautomator dump /sdcard/ui.xml > /dev/null 2>&1
  adb pull /sdcard/ui.xml /tmp/ui.xml > /dev/null 2>&1
  local pos
  pos=$(python3 - "$1" << 'PY'
import re, sys, xml.etree.ElementTree as ET
want = sys.argv[1]
try:
    nodes = list(ET.parse('/tmp/ui.xml').iter('node'))
except Exception:
    sys.exit(0)
for exact in (True, False):
    for n in nodes:
        t, d = n.get('text') or '', n.get('content-desc') or ''
        hit = (t == want or d == want) if exact else (t.startswith(want) or d.startswith(want))
        if hit:
            b = [int(v) for v in re.findall(r'\d+', n.get('bounds', ''))]
            if len(b) == 4 and b[2] > b[0] and b[3] > b[1]:
                print((b[0] + b[2]) // 2, (b[1] + b[3]) // 2)
                sys.exit(0)
PY
  )
  if [ -z "$pos" ]; then
    echo "못 찾음: $1"
    return 1
  fi
  adb shell input tap $pos
  sleep "${2:-5}"
}

shot home
tap_label "앨범" && shot albums
tap_label "Lo-Fi Sketches" && shot album
tap_label "재생" 6 && shot playing
# 미니 플레이어를 누르면 지금 재생 중 화면
tap_label "Tape Hiss Morning" 4 || tap_label "Rooftop Notebook" 4
shot now-playing
adb shell input keyevent 4
sleep 3
tap_label "아티스트" && shot artists
tap_label "라이브" 10 && shot live
tap_label "설정" && shot settings
tap_label "연결" && shot connect
tap_label "라이브러리" && shot settings-library

adb logcat -d | grep -iE "sori|tauri|rust|chromium" | tail -150 > shots/logcat.txt || true
ls -la shots

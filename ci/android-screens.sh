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
# 처음 켤 때 묻는 알림 권한(재생 알림). 시험에서는 미리 허락해 둔다.
adb shell pm grant app.sori.player android.permission.POST_NOTIFICATIONS || true
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

# 화면에서 글자(또는 aria-label)가 $1인 것의 가운데를 누른다. 웹뷰는 접근성 트리를 늦게 만드니
# 못 찾으면 몇 번 다시 본다. 끝내 없으면 1을 돌려준다.
tap_label() {
  local pos=""
  for attempt in 1 2 3 4; do
    adb shell uiautomator dump /sdcard/ui.xml > /dev/null 2>&1
    adb pull /sdcard/ui.xml /tmp/ui.xml > /dev/null 2>&1
    pos=$(find_label "$1" "${3:-first}")
    [ -n "$pos" ] && break
    sleep 2
  done
  if [ -z "$pos" ]; then
    echo "못 찾음: $1"
    return 1
  fi
  adb shell input tap $pos
  sleep "${2:-5}"
}

# $2가 last면 맨 뒤의 것(아래쪽 탭 막대는 화면 맨 끝에 있다)
find_label() {
  python3 - "$1" "$2" << 'PY'
import re, sys, xml.etree.ElementTree as ET
want, which = sys.argv[1], sys.argv[2]
try:
    nodes = list(ET.parse('/tmp/ui.xml').iter('node'))
except Exception:
    sys.exit(0)
if which == 'last':
    nodes.reverse()
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
}

shot home
tap_label "앨범" 5 last && shot albums
tap_label "Lo-Fi Sketches" && shot album
tap_label "재생" 6 && shot playing
# 재생 서비스: 미디어 세션과 알림(잠금 화면·알림 막대의 재생 단추)
adb shell dumpsys media_session | grep -A14 "app.sori.player" | head -40 || true
adb shell cmd statusbar expand-notifications
sleep 3
shot notification
adb shell cmd statusbar collapse
sleep 2
# 미니 플레이어를 누르면 지금 재생 중 화면(시험 곡이 짧아 제목이 그새 바뀌니 이름표로 누른다)
tap_label "지금 재생 중 화면 열기" 4 && shot now-playing
# 지금 재생 중의 ♥(아래쪽 미니 플레이어에도 같은 단추가 있어 맨 뒤의 것)
tap_label "좋아요" 3 last && shot now-playing-liked
adb shell input keyevent 4
sleep 3
# 보관함에 보일 플레이리스트 하나(앞의 곡 여섯)
ids=$(curl -s --max-time 10 "http://127.0.0.1:4380/api/tracks?offset=0&limit=6" | python3 -c "import json,sys; print(json.dumps([t['id'] for t in json.load(sys.stdin)['tracks']]))" || echo "[]")
curl -s --max-time 10 -X POST -H 'content-type: application/json' -d "{\"name\":\"Night Drive Mix\",\"tracks\":$ids}" http://127.0.0.1:4380/api/playlists || true
echo
tap_label "보관함" 5 last && shot collection
tap_label "좋아요" 4 && shot favorites
adb shell input keyevent 4
sleep 3
tap_label "Night Drive Mix" 4 && shot playlist
tap_label "라이브" 10 last && shot live
tap_label "설정" && shot settings
tap_label "연결" && shot connect
tap_label "라이브러리" && shot settings-library
tap_label "정보" 3 && tap_label "기록 보기" 3 && shot logs
curl -s --max-time 10 http://127.0.0.1:4380/api/logs | python3 -c "import json,sys; [print(l['level'], l['message']) for l in json.load(sys.stdin)]" | tail -40 || true

adb logcat -d | grep -iE "sori|tauri|rust|chromium" | tail -150 > shots/logcat.txt || true
ls -la shots

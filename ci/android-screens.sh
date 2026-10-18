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
  dump
  dismiss_anr || true
  n=$((n + 1))
  adb exec-out screencap -p > "shots/android-$(printf %02d $n)-$1.png"
}

dump() {
  adb shell uiautomator dump /sdcard/ui.xml > /dev/null 2>&1
  adb pull /sdcard/ui.xml /tmp/ui.xml > /dev/null 2>&1
}

# 에뮬레이터의 런처가 가끔 "응답하지 않습니다" 창을 띄워 화면을 가린다(소리와는 상관없다).
# 보이면 기다리기를 누른다.
dismiss_anr() {
  grep -q "isn't responding\|응답하지 않" /tmp/ui.xml 2> /dev/null || return 1
  local pos
  pos=$(find_label "Wait" first)
  [ -n "$pos" ] || pos=$(find_label "기다리기" first)
  [ -n "$pos" ] && adb shell input tap $pos
  sleep 2
  dump
  return 0
}

# 화면에서 글자(또는 aria-label)가 $1인 것의 가운데를 누른다. 웹뷰는 접근성 트리를 늦게 만드니
# 못 찾으면 몇 번 다시 본다. 끝내 없으면 1을 돌려준다.
tap_label() {
  local pos=""
  for attempt in 1 2 3 4 5; do
    dump
    dismiss_anr || true
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
# 월드 탭: 처음은 유튜브(키 없음: 구독 피드 안내와 키 넣는 법)
tap_label "월드" 6 last && shot world-youtube
# 키 없이 링크로 연 영상: 진짜 유튜브 공식 플레이어가 앱의 웹뷰 안에서 뜨는지("Me at the zoo")
if tap_label "유튜브 검색" 2 || tap_label "유튜브 검색 또는 링크 붙여 넣기" 2; then
  adb shell input text "https://youtu.be/jNQXAC9IVRw"
  adb shell input keyevent 66
  sleep 12
  shot youtube-watch
  curl -s --max-time 20 "http://127.0.0.1:4380/api/youtube/video/jNQXAC9IVRw" | head -c 600 || true
  echo
  adb shell input keyevent 4
  sleep 3
fi
# 구독(키 없이): 채널을 구독하면 구독 피드에 새 영상이 모인다(유튜브 공식 채널).
curl -s --max-time 10 -X POST -H 'content-type: application/json' -d '{"id":"UCBR8-60-B28hp2BmDPdntcQ","title":"YouTube"}' http://127.0.0.1:4380/api/youtube/subscriptions || true
echo
curl -s --max-time 30 "http://127.0.0.1:4380/api/youtube/feed" | head -c 600 || true
echo
tap_label "밴드" 8 && shot live

# 라이브 아카이브: 공연마다 내려받기를 허락했는지(진짜 archive.org). 그레이트풀 데드의 사운드보드
# 녹음은 듣기만 허락돼 있어야 하고, 킹 기저드는 받을 수 있어야 한다.
for band in "Grateful Dead" "King Gizzard & The Lizard Wizard"; do
  q=$(python3 -c "import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1]))" "$band")
  curl -s --max-time 30 "http://127.0.0.1:4380/api/live/search?q=$q&sort=popular&rows=6" \
    | python3 -c "import json,sys; [print(s['id']) for s in json.load(sys.stdin)['shows']]" > /tmp/ids.txt || true
  while read -r id; do
    curl -s --max-time 30 "http://127.0.0.1:4380/api/live/show/$id" \
      | python3 -c "import json,sys; d=json.load(sys.stdin); t=d['tracks'][0]['format'] if d['tracks'] else {}; print('공연', d['id'], '저장', d['saveable'], d.get('saveBlock') or '', t.get('codec'), sum(x.get('size') or 0 for x in d['tracks']))" || true
  done < /tmp/ids.txt
done
# 받을 수 있는 공연 중 가장 작은 것을 링크로 열고, 저장을 시작했다가 취소한다(휴대폰의 Music 폴더에 쓴다).
pick=$(for id in $(cat /tmp/ids.txt); do curl -s --max-time 30 "http://127.0.0.1:4380/api/live/show/$id"; echo; done \
  | python3 -c "
import json, sys
best = None
for line in sys.stdin:
    try:
        d = json.loads(line)
    except Exception:
        continue
    size = sum(t.get('size') or 0 for t in d.get('tracks', []))
    if d.get('saveable') and size and (best is None or size < best[0]):
        best = (size, d['id'])
print(best[1] if best else '')
" || true)
echo "저장해 볼 공연: $pick"
if [ -n "$pick" ] && { tap_label "archive.org 링크" 2 || tap_label "archive.org 공연 링크로 열기" 2; }; then
  adb shell input text "https://archive.org/details/$pick"
  adb shell input keyevent 66
  sleep 8
  shot live-show
  if tap_label "라이브러리에 저장" 12; then
    shot live-saving
    curl -s --max-time 10 "http://127.0.0.1:4380/api/live/saves/$pick" || true
    echo
    adb shell ls -la "/sdcard/Music/Live\ Archive/*/" || true
    tap_label "취소" 4 && shot live-cancelled
    curl -s --max-time 10 "http://127.0.0.1:4380/api/live/saves/$pick" || true
    echo
    # 취소하면 받던 것을 지운다.
    adb shell ls -la "/sdcard/Music/" || true
  fi
fi
tap_label "설정" && shot settings
tap_label "연결" && shot connect
tap_label "라이브러리" && shot settings-library
tap_label "정보" 3 && tap_label "기록 보기" 3 && shot logs
curl -s --max-time 10 http://127.0.0.1:4380/api/logs | python3 -c "import json,sys; [print(l['level'], l['message']) for l in json.load(sys.stdin)]" | tail -40 || true

adb logcat -d | grep -iE "sori|tauri|rust|chromium" | tail -150 > shots/logcat.txt || true
ls -la shots

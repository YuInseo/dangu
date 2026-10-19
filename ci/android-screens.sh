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
# 구독(키 없이): 채널을 구독하면 구독 피드에 새 영상이 모인다(유튜브 공식 채널).
curl -s --max-time 10 -X POST -H 'content-type: application/json' -d '{"id":"UCBR8-60-B28hp2BmDPdntcQ","title":"YouTube"}' http://127.0.0.1:4380/api/youtube/subscriptions || true
echo
first=$(curl -s --max-time 30 "http://127.0.0.1:4380/api/youtube/feed" | python3 -c "import json,sys; print(json.load(sys.stdin)['items'][0]['title'])" || true)
echo "피드의 첫 영상: $first"
# 탭을 다시 열면 구독을 새로 읽고 피드가 뜬다. 첫 영상을 눌러 진짜 유튜브 공식 플레이어가 앱의 웹뷰 안에서 뜨는지.
tap_label "밴드" 4 && tap_label "유튜브" 10 && shot youtube-feed
if [ -n "$first" ] && tap_label "$first" 15; then
  shot youtube-watch
  adb logcat -d | grep -iE "youtube|iframe|embed|153" | tail -20 || true
  adb shell input keyevent 4
  sleep 3
fi

# 유튜브 백그라운드 재생(설정의 기본값은 켬): 링크로 긴 영상(블렌더 재단의 Big Buck Bunny)을 열고, 홈으로
# 나가도 영상이 이어지는지 본다. 화면의 플레이어가 서버에 알린 상태(external: 재생 중인지)와 미디어 세션,
# 그리고 알림·헤드셋 단추(미디어 키)가 화면의 영상을 멈추고 다시 트는지.
yt_state() {
  curl -sN --max-time 3 http://127.0.0.1:4380/api/player/events 2> /dev/null | sed -n 's/^data://p' | head -1 \
    | python3 -c "import json,sys; d=json.loads(sys.stdin.read() or '{}'); print('$1:', '엔진', d.get('status'), '영상', json.dumps(d.get('external'), ensure_ascii=False))" || true
}
sori_uid=$(adb shell cmd package list packages -U app.sori.player | sed -n 's/.*uid:\([0-9]*\).*/\1/p' | head -1)
echo "소리의 uid: $sori_uid"
yt_sound() {
  adb shell dumpsys audio | grep "u/pid:$sori_uid/" | sed 's/^ *//' | cut -c1-200 | head -5 || true
  adb shell dumpsys media_session | grep -A14 "package=app.sori.player" | grep -E "active=|state=|metadata" | head -4 || true
}
# 웹뷰의 입력 칸은 이름표(aria-label)가 덤프에 나오지 않을 수 있다. 글자·힌트·이름표에 $1이 든 입력 칸,
# 없으면 화면의 $2번째 입력 칸(첫째는 맨 위의 라이브러리 검색).
find_input() {
  python3 - "$1" "$2" << 'PY'
import re, sys, xml.etree.ElementTree as ET
want, nth = sys.argv[1], int(sys.argv[2])
try:
    nodes = list(ET.parse('/tmp/ui.xml').iter('node'))
except Exception:
    sys.exit(0)
edits = [n for n in nodes if n.get('class') == 'android.widget.EditText']
for n in edits:
    print('입력 칸:', {k: n.get(k) for k in ('text', 'hint', 'content-desc', 'bounds')}, file=sys.stderr)
pick = next((n for n in edits if any(want in (n.get(k) or '') for k in ('text', 'hint', 'content-desc'))), None)
if pick is None and len(edits) >= nth:
    pick = edits[nth - 1]
if pick is not None:
    b = [int(v) for v in re.findall(r'\d+', pick.get('bounds', ''))]
    if len(b) == 4:
        print((b[0] + b[2]) // 2, (b[1] + b[3]) // 2)
PY
}
dump
dismiss_anr || true
yt_input=$(find_input "유튜브" 2)
if [ -n "$yt_input" ]; then
  adb shell input tap $yt_input
  sleep 2
  adb shell input text "https://youtu.be/aqz-KE-bpKQ"
  adb shell input keyevent 66
  sleep 25
  shot youtube-bg-watch
  yt_state "앱 안에서"
  yt_sound
  adb shell input keyevent 3
  sleep 15
  yt_state "홈으로 나가고 15초"
  yt_sound
  adb shell cmd statusbar expand-notifications
  sleep 3
  shot youtube-bg-notification
  adb shell cmd statusbar collapse
  sleep 2
  adb shell cmd media_session dispatch pause || adb shell input keyevent 127
  sleep 4
  yt_state "미디어 키: 일시정지"
  adb shell cmd media_session dispatch play || adb shell input keyevent 126
  sleep 8
  yt_state "미디어 키: 재생"
  yt_sound
  adb shell monkey -p app.sori.player -c android.intent.category.LAUNCHER 1
  sleep 5
  shot youtube-bg-back
  # 영상 화면을 떠나면 알림은 엔진으로 돌아간다
  adb shell input keyevent 4
  sleep 3
  yt_state "영상 화면을 떠난 뒤"
fi
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

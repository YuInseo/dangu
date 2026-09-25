#!/usr/bin/env bash
# 모드 스위치를 에뮬레이터에서 실제로 눌러 본다. 화면과 결과는 e2e/ 에 남는다.
set -x
OUT=e2e
mkdir -p "$OUT"
PKG=com.dangu.modes
R="$OUT/result.txt"
: > "$R"
log() { echo "$*" | tee -a "$R"; }

dump() { adb shell uiautomator dump /sdcard/u.xml >/dev/null 2>&1; adb pull /sdcard/u.xml "$OUT/$1" >/dev/null 2>&1; }
shot() { adb exec-out screencap -p > "$OUT/$1"; }
# $1=xml $2=속성 $3=값(부분 일치) → "x y"
find_xy() {
  python3 - "$1" "$2" "$3" <<'PY'
import re, sys
xml = open(sys.argv[1], encoding="utf-8").read()
for m in re.finditer(r'<node [^>]*>', xml):
    node = m.group(0)
    a = re.search(r'%s="([^"]*)"' % re.escape(sys.argv[2]), node)
    b = re.search(r'bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"', node)
    if a and b and sys.argv[3] in a.group(1):
        x1, y1, x2, y2 = map(int, b.groups())
        print((x1 + x2) // 2, (y1 + y2) // 2)
        break
PY
}
tap_text() {
  local xy; xy=$(find_xy "$OUT/$1" "$2" "$3")
  if [ -z "$xy" ] && [ -n "${PANEL_SCROLL:-}" ]; then
    # 엣지 패널은 굴러간다 — 패널 안을 위로 밀고 다시 찾는다
    adb shell input swipe $PANEL_SCROLL; sleep 1; dump "$1"; xy=$(find_xy "$OUT/$1" "$2" "$3")
  fi
  log "  '$3' 위치: ${xy:-없음}"; [ -n "$xy" ] && adb shell input tap $xy
}
top() { adb shell dumpsys activity activities | grep -m1 -E "topResumedActivity|mResumedActivity" | sed 's/^ *//'; }
lock_state() { adb shell dumpsys activity activities | grep -m1 -oE "mLockTaskModeState=[A-Z]+"; }

adb wait-for-device
adb shell settings put global window_animation_scale 0
adb shell settings put global transition_animation_scale 0
adb shell settings put global animator_duration_scale 0
# 화면 고정 기능 켜기(설정 → 보안 → 앱 고정과 같은 값)
adb shell settings put secure lock_to_app_enabled 1 || true
adb shell settings put system lock_to_app_enabled 1 || true

adb install -r modes.apk
adb shell appops set $PKG SYSTEM_ALERT_WINDOW allow
adb shell pm grant $PKG android.permission.POST_NOTIFICATIONS || true

SIZE=$(adb shell wm size | grep -oE '[0-9]+x[0-9]+' | tail -1); W=${SIZE%x*}; H=${SIZE#*x}
DENS=$(adb shell wm density | grep -oE '[0-9]+' | tail -1)
log "화면 ${W}x${H} @${DENS}dpi"
# 막대: 길이 72dp, 굵기 6dp + 누르는 여백 18dp
BTN_PX=$(( 72 * DENS / 160 ))
BTN_W=$(( 24 * DENS / 160 ))
BX=$(( W - BTN_W / 2 ))
BY=$(( (H - BTN_PX) * 40 / 100 + BTN_PX / 2 ))
popup() { adb shell input tap $BX $BY; sleep 2; }
# 오른쪽 엣지 패널 안을 아래에서 위로
PANEL_SCROLL="$(( W - 60 * DENS / 160 )) $(( H * 70 / 100 )) $(( W - 60 * DENS / 160 )) $(( H * 25 / 100 )) 400"

# 1) 설정 → 플로팅 버튼 켜기
adb shell am start -W -n $PKG/.MainActivity
sleep 3
shot 01-settings.png; dump 01.xml
tap_text 01.xml text "플로팅 버튼"
sleep 3
adb shell dumpsys activity services $PKG | grep -q OverlayService && log "① 버튼 서비스: 켜짐" || log "① 버튼 서비스: 안 켜짐"

# 2) 홈 위에 떠 있는 버튼
adb shell input keyevent 3; sleep 2
shot 02-home-with-button.png

# 3) 누르면 모드 목록
popup
shot 03-popup.png; dump 03.xml
grep -q "비밀 바탕화면" "$OUT/03.xml" && log "② 엣지 패널: 모드 목록 보임" || log "② 엣지 패널: 안 보임"

# 4) 비밀 바탕화면 — 텅 빈 새 홈을 진짜 홈처럼 편집
tap_text 03.xml text "비밀 바탕화면"; sleep 3
shot 04-secret-empty.png; dump 04.xml
log "③ 비밀 바탕화면 → $(top)"
grep -q "텅 빈 새 바탕화면" "$OUT/04.xml" && log "   처음엔 비어 있음: 예" || log "   처음엔 비어 있음: 아니오"
# 빈 곳 길게 → 편집 모드
adb shell input swipe $(( W / 2 )) $(( H / 2 )) $(( W / 2 )) $(( H / 2 )) 900; sleep 2
shot 04b-edit-mode.png; dump 04b.xml
grep -q "바탕화면 편집" "$OUT/04b.xml" && log "   빈 곳 길게 눌러 편집 모드: 됨" || log "   빈 곳 길게 눌러 편집 모드: 안 됨"
tap_text 04b.xml text "앱"; sleep 3
dump 04c.xml
for app in Settings Camera Gallery Phone Contacts Messaging; do
  XY=$(find_xy "$OUT/04c.xml" text "$app"); [ -n "$XY" ] && adb shell input tap $XY && sleep 0.3
done
dump 04d.xml
tap_text 04d.xml text "추가 "; sleep 3
shot 04e-apps-added.png; dump 04e.xml
log "   앱 추가 뒤 칸: $(grep -oE 'content-desc="(Settings|Camera|Gallery|Phone|Contacts|Messaging)"' "$OUT/04e.xml" | tr '\n' ' ')"
# 첫 앱을 둘째 앱 위로 끌면 폴더
A1=$(find_xy "$OUT/04e.xml" content-desc "Camera"); A2=$(find_xy "$OUT/04e.xml" content-desc "Phone")
log "   Camera: ${A1:-없음} → Phone: ${A2:-없음}"
if [ -n "$A1" ] && [ -n "$A2" ]; then
  adb shell input draganddrop $A1 $A2 1500; sleep 3
fi
shot 04f-folder.png; dump 04f.xml
grep -q 'content-desc="폴더"' "$OUT/04f.xml" && log "   앱 위에 놓아 폴더 만들기: 됨" || log "   앱 위에 놓아 폴더 만들기: 안 됨"
# 한 앱을 위의 🗑로 끌어 삭제
A3=$(find_xy "$OUT/04f.xml" content-desc "Gallery")
if [ -n "$A3" ]; then adb shell input draganddrop $A3 $(( W / 2 )) $(( 60 * DENS / 160 + 30 )) 1500; sleep 3; fi
dump 04g.xml
grep -q 'content-desc="Gallery"' "$OUT/04g.xml" && log "   🗑로 끌어 삭제: 안 됨" || log "   🗑로 끌어 삭제: 됨"
tap_text 04g.xml text "완료"; sleep 2
shot 04h-edited-home.png
dump 04h.xml
A4=$(find_xy "$OUT/04h.xml" content-desc "Contacts")
if [ -n "$A4" ]; then
  set -- $A4
  adb shell input swipe $1 $2 $1 $2 900; sleep 2
  shot 04i-app-menu.png; dump 04i.xml
  grep -q "앱 정보" "$OUT/04i.xml" && log "   앱 길게 누르면 메뉴: 됨" || log "   앱 길게 누르면 메뉴: 안 됨"
  grep -q "바탕화면 편집" "$OUT/04i.xml" && grep -q '"완료"' "$OUT/04i.xml" && log "   (앱 길게 눌렀는데 편집 모드로 바뀜 — 잘못)"
  adb shell input keyevent 4; sleep 1
  # 편집 모드가 아닌 채로 끌어 옮기기
  adb shell input draganddrop $1 $2 $(( W / 2 )) $(( H * 55 / 100 )) 1500; sleep 2
  shot 04j-moved-without-edit.png
fi

# 5) 다시 버튼 → 기본 바탕화면
popup; dump 05.xml
tap_text 05.xml text "기본 바탕화면"; sleep 3
shot 05-default-home.png
log "④ 기본 바탕화면 → $(top)"
log "   기본 홈 앱 그대로?: $(adb shell cmd package resolve-activity --brief -a android.intent.action.MAIN -c android.intent.category.HOME | tail -1)"

# 6) 앱 고정 (일반 모드: 시스템 확인 창)
popup; dump 06.xml
tap_text 06.xml text "설정 앱 고정"; sleep 3
shot 06-pin-confirm.png; dump 06b.xml
# 시스템의 "App is pinned" 창은 별도 창이라 dump에 안 잡힌다 — 화면의 "GOT IT" 자리(오른쪽 아래)를 누른다.
adb shell input tap $(( W * 73 / 100 )) $(( H * 944 / 1000 ))
sleep 4
shot 07-pinned.png
log "⑤ 화면 고정 상태: $(lock_state) / 위: $(top)"
adb shell input keyevent 3; sleep 2
shot 08-pinned-after-home.png
log "   홈 눌러도: $(lock_state) / 위: $(top)"
popup
shot 09-popup-while-pinned.png
adb shell input keyevent 4; sleep 1
adb shell am task lock stop; sleep 2
log "   해제 뒤: $(lock_state)"

# 7) Device Owner 잠금(확인 창 없음)
adb shell dpm set-device-owner $PKG/.AdminReceiver | tee -a "$R"
adb shell input keyevent 3; sleep 2
popup; dump 10.xml
tap_text 10.xml text "설정 앱 고정"; sleep 4
shot 10-kiosk-locked.png
log "⑥ 기기 소유자 잠금: $(lock_state) / 위: $(top)"
adb shell input keyevent 3; sleep 2
log "   홈 눌러도: $(lock_state) / 위: $(top)"
adb shell input keyevent 4; sleep 2
shot 11-kiosk-host.png; dump 11.xml
tap_text 11.xml text "고정 해제"; sleep 2
log "   [고정 해제] 뒤: $(lock_state)"

# 8) 설정에서 끌어서 순서 바꾸기
adb shell am start -W -n $PKG/.MainActivity; sleep 3
adb shell input swipe $(( W / 2 )) $(( H * 3 / 4 )) $(( W / 2 )) $(( H / 4 )) 300; sleep 1
dump 12.xml
XY=$(find_xy "$OUT/12.xml" content-desc "끌어서 순서 바꾸기: 기본 바탕화면")
log "⑦ 끌기 손잡이: ${XY:-없음}"
before=$(grep -oE 'text="[^"]*(바탕화면|업무 프로필|고정)"' "$OUT/12.xml" | tr '\n' ' ')
if [ -n "$XY" ]; then
  set -- $XY
  ROW=$(( 60 * DENS / 160 ))
  adb shell input swipe $1 $2 $1 $(( $2 + ROW * 2 + ROW / 4 )) 1500; sleep 2
fi
shot 12-reordered.png; dump 13.xml
after=$(grep -oE 'text="[^"]*(바탕화면|업무 프로필|고정)"' "$OUT/13.xml" | tr '\n' ' ')
log "   전: $before"
log "   후: $after"

adb shell input keyevent 3; sleep 1
popup
shot 13-popup-new-order.png
adb shell input keyevent 4; sleep 1

# 9) 모드를 많이 만들어 원형 메뉴 돌리기 (한 번에 5개, 모드 8개 + 설정)
adb shell am start -W -n $PKG/.MainActivity; sleep 2
for i in 1 2 3 4; do
  adb shell input swipe $(( W / 2 )) $(( H * 3 / 4 )) $(( W / 2 )) $(( H / 3 )) 300; sleep 1
  dump add.xml; tap_text add.xml text "모드 추가"; sleep 2
  dump add2.xml; tap_text add2.xml text "기본 바탕화면"; sleep 1
  tap_text add2.xml text "이름"; sleep 1
  adb shell input text "extra$i"; sleep 1
  dump add3.xml; tap_text add3.xml text "저장"; sleep 2
done
adb shell input swipe $(( W / 2 )) $(( H / 3 )) $(( W / 2 )) $(( H * 3 / 4 )) 300; sleep 1
adb shell input swipe $(( W / 2 )) $(( H / 3 )) $(( W / 2 )) $(( H * 3 / 4 )) 300; sleep 1
dump style.xml; tap_text style.xml text "원형"; sleep 1
adb shell input keyevent 3; sleep 1
popup
shot 14-wheel-before.png; dump 14.xml
B14=$(grep -oE 'content-desc="[^"]+"[^>]*bounds="[^"]+"' "$OUT/14.xml" | grep -vE '닫기' | tr '\n' ' '); log "⑧ 돌리기 전 보이는 거품: $B14"
# 버튼 둘레로 원을 그리며 끌기(아래 → 위: 처음엔 맨 위 항목이 위 끝이라, 위로 돌려야 다음 것들이 나온다)
R=$(( 140 * DENS / 160 ))
adb shell input swipe $(( BX - R )) $(( BY + R / 2 )) $(( BX - R / 2 )) $(( BY - R )) 800; sleep 2
shot 15-wheel-rotated.png; dump 15.xml
B15=$(grep -oE 'content-desc="[^"]+"[^>]*bounds="[^"]+"' "$OUT/15.xml" | grep -vE '닫기' | tr '\n' ' ')
log "   돌린 뒤: $B15"
[ "$B14" != "$B15" ] && log "   돌리기: 됨" || log "   돌리기: 그대로"
adb shell input keyevent 4; sleep 1

# 10) 왼쪽으로: 버튼을 끌어 화면 가운데를 넘겨 놓기
adb shell input swipe $BX $BY $(( W / 5 )) $BY 600; sleep 2
shot 16-left-side.png
adb shell input tap $(( BTN_W / 2 )) $BY; sleep 2
shot 17-left-menu.png
adb shell input keyevent 4

adb logcat -d | grep -A30 "FATAL EXCEPTION" > "$OUT/crash.txt" || echo "크래시 없음" > "$OUT/crash.txt"
[ -s "$OUT/crash.txt" ] || echo "크래시 없음" > "$OUT/crash.txt"
ls -la "$OUT"
exit 0

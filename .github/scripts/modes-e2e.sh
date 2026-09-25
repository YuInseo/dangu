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
tap_text() { local xy; xy=$(find_xy "$OUT/$1" "$2" "$3"); log "  '$3' 위치: ${xy:-없음}"; [ -n "$xy" ] && adb shell input tap $xy; }
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
BTN_PX=$(( 48 * DENS / 160 ))
BX=$(( W - BTN_PX * 3 / 8 ))
BY=$(( (H - BTN_PX) * 40 / 100 + BTN_PX / 2 ))
popup() { adb shell input tap $BX $BY; sleep 2; }

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
grep -q "비밀 바탕화면" "$OUT/03.xml" && log "② 팝업: 모드 목록 보임" || log "② 팝업: 안 보임"

# 4) 비밀 바탕화면
tap_text 03.xml text "비밀 바탕화면"; sleep 3
shot 04-secret-desktop.png
log "③ 비밀 바탕화면 → $(top)"

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
for t in "Got it" "GOT IT" "OK" "확인" "고정"; do
  XY=$(find_xy "$OUT/06b.xml" text "$t"); if [ -n "$XY" ]; then log "  확인 단추 '$t'"; adb shell input tap $XY; break; fi
done
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
adb shell input keyevent 4

adb logcat -d | grep -A30 "FATAL EXCEPTION" > "$OUT/crash.txt" || echo "크래시 없음" > "$OUT/crash.txt"
[ -s "$OUT/crash.txt" ] || echo "크래시 없음" > "$OUT/crash.txt"
ls -la "$OUT"
exit 0

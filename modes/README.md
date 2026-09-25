# 모드 스위치

화면 오른쪽 끝의 플로팅 버튼 하나로 **"화면 모드(환경)"를 즉시 바꾸는** 안드로이드 앱.
기본 바탕화면 ↔ 비밀 바탕화면 ↔ 업무 프로필 ↔ 특정 앱 고정을 오간다.

**원칙: 기본 홈 앱(Default Launcher)은 절대 건드리지 않는다.** 이 앱은 `CATEGORY_HOME`을
선언하지 않으므로 "기본 홈 앱" 후보에조차 오르지 않고, 홈 설정 화면을 여는 코드도 없다.
바탕화면 전환은 "홈을 바꾸는 것"이 아니라 **"홈 위에 다른 환경을 띄우는 것"**으로 구현한다.

---

## 1. 권한

| 권한 | 왜 필요한가 | 받는 방법 |
|---|---|---|
| `SYSTEM_ALERT_WINDOW` | 모든 앱 위에 떠 있는 플로팅 버튼과 모드 목록 팝업 (`TYPE_APPLICATION_OVERLAY`) | 특별 권한 — 설정 화면(`ACTION_MANAGE_OVERLAY_PERMISSION`)으로 보내 사용자가 켬 |
| `FOREGROUND_SERVICE` + `FOREGROUND_SERVICE_SPECIAL_USE` | 버튼을 그리는 서비스가 백그라운드에서 죽지 않게. Android 14부터 포그라운드 서비스는 종류를 밝혀야 하고, "화면 위 버튼"은 정해진 종류가 없어 `specialUse` + 사유 속성(`PROPERTY_SPECIAL_USE_FGS_SUBTYPE`)으로 선언 | 일반 권한(설치 시 자동) |
| `POST_NOTIFICATIONS` | 포그라운드 서비스의 상시 알림(버튼 숨기기/설정 바로가기). Android 13+ | 런타임 요청 |
| `RECEIVE_BOOT_COMPLETED` | 재부팅 후 버튼 자동 복원 | 일반 권한 |
| `USE_BIOMETRIC` | 비밀 바탕화면 진입 시 지문/얼굴(또는 기기 PIN) 확인 | 일반 권한 |
| `<queries>` (MAIN/LAUNCHER) | 앱 목록(고정할 앱, 비밀 바탕화면에 넣을 앱)을 고르기 위해. Android 11 패키지 가시성. `QUERY_ALL_PACKAGES`는 쓰지 않는다 — 실행 가능한 앱만 보이면 충분하고, 스토어 심사에서 설명할 필요도 없다 | 매니페스트 선언만 |
| (선택) Device Owner | 확인 창 없는 **진짜 잠금(Lock Task, 키오스크)**. 일반 사용자 폰에선 불가능하고 공장 초기화 직후 `adb`로만 지정할 수 있어 "고급" 경로로 둔다 | `adb shell dpm set-device-owner com.dangu.modes/.AdminReceiver` |

쓰지 **않는** 것: 접근성 서비스(화면 감시·자동 조작 — 목적 외 사용), `BIND_DEVICE_ADMIN`을 통한 강제
정책, 홈 앱 지정(`RoleManager.ROLE_HOME`), `QUERY_ALL_PACKAGES`, `PACKAGE_USAGE_STATS`.

## 2. 시스템 흐름

```
┌────────────── 설정 앱(MainActivity) ───────────────┐
│ 권한 확인 · 버튼 켜기 · 위치/크기/투명도 · 모드 목록 편집(드래그)│
└──────────────┬───────────────────────────────┘
               │ SharedPreferences (Store) — 바뀌면 리스너로 즉시 전달
               ▼
┌───────── OverlayService (포그라운드, specialUse) ─────────┐
│ WindowManager.addView(TYPE_APPLICATION_OVERLAY)              │
│  ├ 버튼 창: 오른쪽 끝, 세로로 끌어 이동, 누르면 ↓             │
│  └ 팝업 창: 모드 목록 (바깥 누르면 닫힘)                       │
└──────────────┬───────────────────────────────┘
               │ 선택
               ▼
        ModeLauncher.launch(mode)
   ┌───────────┼───────────────┬────────────────┬───────────────┐
   ▼           ▼               ▼                ▼               ▼
 기본 바탕화면   비밀 바탕화면       업무 프로필         앱 실행          앱 고정
 HOME 인텐트    SecretDesktop    LauncherApps       런처 인텐트      PinHostActivity
 (사용자의     Activity         (다른 사용자          (NEW_TASK)      startLockTask()
  원래 홈)     생체 인증·FLAG_   프로필의 앱 실행)                    → 대상 앱 실행
              SECURE·최근 앱
              목록에서 숨김
```

### 기본 홈을 안 바꾸고 "바탕화면이 바뀌는" 가장 우아한 방법

1. **기본 바탕화면으로** = `Intent(ACTION_MAIN).addCategory(CATEGORY_HOME)`. 누가 홈인지는 시스템이
   정한다 — 우리는 "홈으로 가 달라"고만 한다. 사용자가 고른 런처가 그대로 뜬다.
2. **비밀/다른 바탕화면** = 홈이 아닌 **전용 태스크의 전체 화면 액티비티**.
   - `taskAffinity`를 따로 두고 `launchMode="singleTask"` — 한 번 만든 "바탕화면"이 재사용되어,
     몇 번을 오가도 쌓이지 않고 즉시 뜬다.
   - `android:windowShowWallpaper="true"` — 사용자의 실제 배경화면 위에 그려져 진짜 홈처럼 보인다.
   - `excludeFromRecents` + `FLAG_SECURE` — 최근 앱 목록과 스크린샷에 남지 않는다(프라이버시).
   - 홈 버튼을 누르면 시스템은 **원래 홈으로** 간다. 이게 정확히 "하이재킹하지 않는다"의 뜻이다.
     돌아오려면 플로팅 버튼 → 비밀 바탕화면.
3. **업무 프로필** = `LauncherApps.getActivityList(null, 업무 UserHandle)`로 그 프로필의 앱을 모아
   보여 주고 `LauncherApps.startMainActivity()`로 실행. Work Profile이 없으면 만드는 방법(회사 MDM,
   Shelter/Island 같은 앱)을 안내만 한다 — 프로필을 만드는 것 자체는 Device Policy 권한이 필요한
   별개의 일이라 이 앱의 범위 밖.

## 3. 앱 고정 — 두 단계

| | 일반 폰 (기본) | Device Owner로 지정된 폰 (고급) |
|---|---|---|
| 방식 | 화면 고정(Screen Pinning, `LOCK_TASK_MODE_PINNED`) | 잠금 작업(Lock Task, `LOCK_TASK_MODE_LOCKED`) |
| 확인 창 | 시스템이 "앱을 고정할까요?"를 묻는다(끌 수 없음) | 없음 |
| 고정 대상 | `PinHostActivity`의 태스크. 대상 앱을 **같은 태스크 안으로**(NEW_TASK 없이) 연다 | `setLockTaskPackages()`에 넣은 앱 그 자체 (`ActivityOptions.setLockTaskEnabled(true)`) |
| 한계 | 대상 앱의 첫 화면이 `singleTask`/`singleInstance`면 새 태스크가 필요해 시스템이 막는다 → 안내 후 해제 | 없음 |
| 빠져나가기 | 뒤로+최근 앱 버튼 길게(설정의 "해제 시 PIN 요구"와 함께 쓰면 안전) · 앱의 "고정 해제" | 앱의 "고정 해제"(`stopLockTask`)만 |

일반 폰에서 "화면 고정" 기능이 꺼져 있으면 `startLockTask()`는 아무 일도 하지 않는다 — 앱이 이를
감지해 설정(보안 → 앱 고정)으로 안내한다. 고정 중에는 플로팅 버튼이 다른 모드로 보내지 않는다.

---

## 코드 지도

| 파일 | 역할 |
|---|---|
| `OverlayService.kt` | 플로팅 버튼·팝업 창. `Store`의 변경을 듣고 `updateViewLayout()`으로 즉시 반영 |
| `Store.kt` / `Mode.kt` | 버튼 설정과 모드 목록(JSON) 저장, 변경 알림 |
| `ModeLauncher.kt` | 모드 → 실제 전환 |
| `PinHostActivity.kt` | 앱 고정(두 단계 모두) |
| `SecretDesktopActivity.kt` | 비밀 바탕화면(생체 인증, 최근 앱·스크린샷 숨김) |
| `WorkDesktopActivity.kt` | 업무 프로필 앱 |
| `ui/SettingsScreen.kt` | 설정 — 권한, 버튼 위치·크기, 모드 드래그 정렬·편집 |

## 빌드와 확인

`modes/`가 바뀌면 GitHub Actions(`.github/workflows/modes.yml`)가 APK를 만들어
`modes-latest` 릴리스에 올리고, 에뮬레이터에서 실제로 눌러 본다 — 버튼 → 팝업 → 비밀 바탕화면 →
기본 바탕화면 → 화면 고정(확인 창) → Device Owner 잠금 → 드래그로 순서 바꾸기. 결과 화면은
`modes-e2e` 릴리스에 남는다.

최소 Android 10 (API 29).

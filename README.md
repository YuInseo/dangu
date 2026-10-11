# 동행

교통약자 승하차 도움 쪽지 앱. 앱은 [`donghaeng/`](donghaeng/) 폴더에 있습니다.

**APK 받기:** [donghaeng-ci.apk](https://github.com/YuInseo/dangu/releases/download/donghaeng-ci/donghaeng-ci.apk)

교통약자 승하차 도움 쪽지를 팀이 같이 보는 앱. 종이 쪽지를 들고 다니던 것을 폰으로 옮겼습니다.

종이 쪽지는 한 장이라 그걸 쥔 사람만 볼 수 있었습니다. 동행에서는 한 사람이 적으면 팀 모두의
폰에 바로 뜨고, 누가 맡았는지·끝났는지도 그 자리에서 같이 바뀝니다.

- **쪽지 칸은 종이 양식 그대로** — 출발/도착, 시간, 열차번호, 홈, 구간(○○행 / ○○에서 온),
  좌석번호(호차-좌석), 내용(휠필·리프트·시각·무표), 그리고 손으로 적던 만날 곳·연락처·메모.
- **쪽지판** — 하루치 쪽지가 열차 시각 순서로. 남은 시간("12분 후")이 보이고, 10분 안쪽이면
  빨갛게 바뀝니다. 끝난 쪽지는 아래로 내려가고, 숨길 수도 있습니다.
- **맡기 → 완료** — "내가 맡기"를 누르면 팀 모두의 화면에 내 이름이 붙습니다. 두 사람이 같은
  고객을 모시러 가는 일이 없습니다.
- **새 쪽지 알림** — 다른 사람이 쪽지를 올리면 진동과 함께 위에 알림이 뜨고, 그 쪽지에 노란
  테두리가 생깁니다.
- **공유** — 앱이 없는 사람(다른 역, 차장)에게는 쪽지를 글로 만들어 카톡 등으로 보냅니다.
- **오프라인** — 신호가 끊긴 곳에서 적은 쪽지도 기기에 먼저 남고, 연결되면 올라갑니다.
- **개인정보** — 쪽지는 이틀이 지나면 자동으로 지워집니다(오늘·어제 것만 남음).

## 쓰는 법

1. 처음 켜면 이름을 적고, **새 팀 만들기**(역에 한 사람) 또는 **코드로 들어가기**.
2. 팀을 만든 사람은 설정 화면의 **코드 보내기**로 여덟 글자 코드를 동료에게 보냅니다.
3. 아래의 **+ 출발 쪽지 / + 도착 쪽지**로 적습니다. 시간은 `1713`만 쳐도 `17:13`이 됩니다.

## 설정해야 하는 것 — Firebase 콘솔

동행은 전용 Firebase 프로젝트 **`motion-8b939`("With")**를 씁니다. 당구 점수판(`moneywalk-551ca`)과
로그인·데이터가 섞이지 않습니다. 그 프로젝트에 해 둘 것:

1. **프로젝트 설정 → 내 앱**에 Android 앱 `com.donghaeng.app`과 서명 키의 SHA-1
   (`D4:67:06:0E:4C:1D:99:8D:5F:01:EA:B2:25:B0:17:31:3F:0A:5C:7A`, CI 업로드 키)이 등록돼 있어야
   합니다 — 앱 안의 구글 로그인 창이 이걸로 열립니다. (등록돼 있음)
2. **Authentication → 로그인 방법**에서 **Google**을 켭니다. "로그인 없이 시작"을 쓰려면 **익명**도.
3. **Firestore Database**를 만들고, **규칙**에 `donghaeng/firestore.rules`를 통째로 붙여넣습니다.

앱 설정값은 `donghaeng/graft.config.ts`(웹 SDK)와
`donghaeng/mobile/overlay/android/app/src/main/res/values/firebase.xml`(안드로이드)에 있고, 둘 다 그
앱의 `google-services.json`에서 옮긴 값입니다.

색인은 따로 만들 필요가 없습니다 — 쪽지는 날짜 하나로만 묻습니다.

## 개발

이 폴더는 당구 점수판과 별개의 Graft 앱입니다. 프레임워크를 옆에 두고:

```bash
cd donghaeng
npm install            # graft는 상위 워크스페이스나 file: 의존으로
npx graft dev --port 3300
npx tsc --noEmit
node scripts/make-icons.mjs   # 아이콘 다시 만들기
```

## 안드로이드 앱과 자동 업데이트

`donghaeng` 브랜치에 `donghaeng/`가 바뀐 푸시가 들어오면 `.github/workflows/donghaeng.yml`이
APK와 화면 묶음을 지어 **`donghaeng-ci` 프리릴리스**에 올립니다. 판 번호는 `0.1.<빌드 번호>`입니다.

- **처음 한 번만** 위의 APK 링크로 설치합니다.
- 그다음부터는 **앱이 스스로 업데이트합니다.** 켤 때 새 판이 있으면 화면을 조용히 받아 두고
  "새 버전 준비됨"을 띄웁니다 — 다음에 켜면 적용되고, **지금 적용**을 눌러도 됩니다.
- 플러그인이나 설정처럼 앱 껍데기까지 바뀐 판이면 **받기** 버튼이 뜨고, 받은 APK를 눌러
  설치합니다(안드로이드가 사이드로딩 앱의 조용한 설치를 막기 때문에 이것만은 한 번 눌러야 합니다).
- 새 화면이 앱을 띄우지 못하면 다음 실행에서 이전 화면으로 저절로 돌아갑니다.
- 설정 → 업데이트에서 지금 판을 보고 직접 확인할 수 있습니다.

서명은 점수판과 같은 시크릿(`ANDROID_KEYSTORE_BASE64` 등)을 씁니다. 업데이트는 `donghaeng-ci`
릴리스만 보므로 점수판의 업데이트(`v*` 릴리스)와 섞이지 않습니다. 패키지 이름도
따로(`com.donghaeng.app`)라서 한 폰에 점수판과 같이 깔립니다.

## 코드가 어디 있는지

| 파일 | 무엇 |
| --- | --- |
| `donghaeng/lib/slip.ts` | 쪽지의 모양, 시각·날짜 계산, 공유 글. UI도 Firebase도 모릅니다 |
| `donghaeng/lib/firebase.ts` | 구글 로그인, 팀 만들기·들어가기, 쪽지 읽기·쓰기·지우기 |
| `donghaeng/lib/session.ts` | 이 폰의 이름과 팀 |
| `donghaeng/lib/platform.ts` | 기기 저장소·진동·공유 |
| `donghaeng/lib/update.ts` | 자동 업데이트 — 화면 갈아끼우기와 APK 안내 |
| `donghaeng/app/home.tsx` | 첫 화면 — 이름과 팀이 없으면 설정, 있으면 쪽지판 |
| `donghaeng/app/board.tsx` | 쪽지판 |
| `donghaeng/app/slip-card.tsx` | 목록의 쪽지 한 장 |
| `donghaeng/app/slip-sheet.tsx` | 펼친 쪽지 — 맡기, 완료, 공유, 고치기 |
| `donghaeng/app/slip-editor.tsx` | 쪽지 쓰기·고치기 |
| `donghaeng/app/settings/settings-panel.tsx` | 팀 코드, 구성원, 내 이름, 나가기 |

---

이 저장소에는 당구 점수판도 함께 있습니다 — [BILLIARD.md](BILLIARD.md).

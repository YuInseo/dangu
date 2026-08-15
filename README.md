# 당구 점수판

4구 · 3구 · 포켓볼 점수판. 한 코드로 웹과 안드로이드 앱이 나옵니다.
[Graft](https://github.com/YuInseo/graft)로 만들었습니다 — 프레임워크는 별도 저장소이고,
CI는 빌드할 때 그 저장소를 가져와 `file:` 의존으로 설치합니다. 로컬에서는 상위 폴더의
npm 워크스페이스가 같은 일을 하므로, 고친 프레임워크가 곧바로 반영됩니다.

- **왼쪽 흰 공, 오른쪽 노란 공.** 각자 자기 쪽에 큰 점수와 `+1 +2 +3` / `−1 −2 −3`,
  그리고 그보다 큰 값을 넣을 직접 입력이 있습니다. 자기 색 판의 빈 곳을 아무 데나
  눌러도 1점입니다.
- **아래 푸터**에 경과 시간, 이닝, 에버, 턴 넘김, 되돌리기, 일시정지, 종료.
- **핸디**를 사람마다 따로. 4구는 **마지막 쿠션**까지 — 목표 점수를 채운 뒤 쿠션으로
  몇 점을 더 쳐야 하는지 정하면, 그 구간에 들어간 순간 점수판이 0부터 다시 세고
  "쿠션"이라고 크게 알려 줍니다.
- **통계**: 오늘 / 이번 달 게임 수, 승·패, 승률, 친 시간, 상대별 승률, 종목별 에버,
  연승, 그리고 게임 하나하나의 상세. 끝난 기록은 나중에 고칠 수 있습니다 — 종목,
  이름, 점수, 목표, 이닝, 시간, 승자까지.
- **로그인하면 기기끼리 알아서 같아집니다.** 같은 계정이면 폰에서 친 판이 태블릿에
  바로 뜨고, 한쪽에서 지운 판은 다른 쪽에서도 사라지고, 내 이름·다니는 당구장·늘 치는
  종목 같은 설정도 함께 따라갑니다. 로그인한 사람의 기록은 묻지 않고 계정으로 옮깁니다 —
  "이 기기에만"을 직접 고른 사람만 그대로 둡니다. 어느 쪽이든 게임은 항상 기기에 먼저
  저장되므로 네트워크가 없어도 끝까지 돌아갑니다.
- **업데이트는 대개 조용히 끝납니다.** 앱이 켜질 때 새 웹 번들을 받아 두고 다음
  실행에 적용합니다 — 누를 것도, 기다릴 것도 없습니다. 안드로이드 설치 화면이 뜨는
  것은 앱 껍데기까지 바뀐 버전뿐이고, 그때도 APK는 미리 받아 둡니다.

---

## 개발

```bash
npm run dev            # http://localhost:3200
npm run build          # 웹 + 모바일 페이로드
npx tsc --noEmit       # 타입 검사
```

아이콘을 다시 만들려면 `node scripts/make-icons.mjs`.

## 안드로이드 앱

```bash
node ../graft/bin/graft.js build --target mobile --platform android
cd .graft/mobile
npm pkg set "dependencies.@capgo/capacitor-updater=6.13.2"   # 아래 참고
npm install && npm run add:android                           # 최초 한 번
npx cap sync android
cd android && ./gradlew assembleDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

업데이터 플러그인만 버전을 못박습니다. 최신 6.x는 `androidx.work` 2.10을 끌고 오고 그건
`compileSdk 35`를 요구하는데, 이 앱은 34에 머물러 있기 때문입니다(아래 targetSdk 이야기와
같은 이유). 6.13.2가 `work` 2.9.1을 쓰는 마지막 판입니다. CI도 같은 값을 씁니다.

`.graft/mobile/`은 전부 생성물입니다 — 손으로 고치지 마세요. 안드로이드에 직접 넣을
것이 생기면 `mobile/native/`(코틀린 플러그인)와 `mobile/native/android/inject/`
(매니페스트·Gradle 조각)에 두면 빌드가 옮겨 줍니다.

---

## 설정해야 하는 것 — Firebase

이 저장소는 **`moneywalk-551ca` 프로젝트에 이미 연결되어 있습니다.** 저장소에 들어 있는
것은 이렇습니다:

| 어디 | 무엇 |
| --- | --- |
| `graft.config.ts`의 `env` | 웹 설정 여섯 값(기본값). 시크릿이나 `.env.local`이 있으면 그쪽이 이깁니다 |
| `mobile/overlay/android/app/google-services.json` | 콘솔에서 받은 안드로이드 설정 원본 |
| `mobile/overlay/…/res/values/firebase.xml` | 그 json에서 옮긴 안드로이드 리소스. 네이티브 로그인이 읽는 쪽 |
| `firestore.rules` | 자기 문서만 자기가 읽고 쓰는 규칙 — 기록, 지운 기록, 설정 세 갈래 |

값이 저장소에 그대로 적혀 있어도 되는 이유는 규칙입니다. Firebase 웹 설정은 "어느
프로젝트인지"를 말할 뿐이고, 누가 무엇을 읽고 쓸 수 있는지는 `firestore.rules`가
정합니다. 그래서 이 값들이 공개된 상태를 전제로 규칙을 짭니다.

**콘솔에서 아직 해야 하는 것** — 코드로는 할 수 없는 것들입니다:

1. **Authentication → Sign-in method**에서 **Google**과 **이메일/비밀번호**를 켭니다.
   이메일 쪽이 필요한 이유는 앱 안에서입니다 — 구글 로그인은 네이티브 창을 띄우고 그
   창은 APK 안의 설정을 읽으므로, 새 APK를 깔기 전에는 켜지지 않습니다. 이메일 로그인은
   전부 자바스크립트라 조용한 웹 업데이트만으로 그 자리에서 동작합니다.
2. **Firestore Database**를 만들고 규칙에 `firestore.rules`의 세 블록을 넣습니다 —
   `users/{uid}/games/{gameId}`(기록), `users/{uid}/trash/{gameId}`(지운 기록의 이름표),
   `users/{uid}/prefs/{docId}`(계정을 따라다니는 설정). ⚠️ 이 프로젝트를 다른 앱과 함께
   쓰고 있다면 파일을 통째로 붙여넣지 마세요 — 맨 아래
   `match /{document=**} { allow read, write: if false; }`가 그 앱의 컬렉션까지 닫아
   버립니다. 블록만 기존 규칙에 더하세요.

   > 이미 쓰고 있던 프로젝트라면 `trash`와 `prefs` 블록을 더하는 것을 잊지 마세요. 규칙이
   > 없으면 그 두 가지는 조용히 막히고, 기록만 흐르는 반쪽짜리 상태가 됩니다 — 한쪽에서
   > 지운 판이 다른 기기에서 되살아나는 것이 그 모습입니다.
3. **안드로이드 앱(`com.dangu.score`)에 서명 키의 SHA-1 지문을 등록**합니다. 이게 없으면
   앱에서 구글 로그인이 `DEVELOPER_ERROR`로 끝납니다. 릴리스 APK는 CI의 업로드 키로
   서명되므로 그 키의 지문이 필요합니다:

   ```bash
   keytool -list -v -keystore release.keystore -alias upload
   # 디버그 빌드로 시험할 때는 디버그 키의 지문도 같이 등록합니다
   keytool -list -v -keystore ~/.android/debug.keystore -alias androiddebugkey -storepass android -keypass android
   ```

다른 프로젝트로 옮기려면 콘솔에서 받은 `google-services.json`을
`mobile/overlay/android/app/`에 덮어쓰고, 그 안의 값들을 `firebase.xml`과
`graft.config.ts`에 옮기면 됩니다. 저장소에 값을 남기고 싶지 않다면 대신 시크릿
(`FIREBASE_*`, `GOOGLE_SERVICES_JSON`)에 넣으세요 — 그쪽이 언제나 이깁니다.

앱에서의 구글 로그인은 `@capacitor-firebase/authentication`이 네이티브 창을 띄우고,
웹에서는 팝업을 씁니다 — 구글이 WebView 안의 OAuth를 거절하기 때문입니다. 호출하는
코드는 `lib/firebase.ts` 한 곳뿐입니다.

## 설정해야 하는 것 — GitHub 릴리스와 인앱 업데이트

1. 이 폴더를 GitHub 저장소로 올립니다.
2. `.env.local`(또는 CI 환경변수)에 저장소 이름을 넣습니다:

   ```
   GRAFT_PUBLIC_GITHUB_REPO=your-name/billiard
   ```

   워크플로는 이 값을 `github.repository`로 자동으로 채우므로, CI가 만든 APK는
   자기 저장소를 압니다.
3. 저장소 **Settings → Secrets and variables → Actions**에 넣을 것들 (전부 선택
   사항이며, 없으면 그 부분만 꺼진 채로 빌드됩니다):

   | 시크릿 | 쓰임 |
   | --- | --- |
   | `FIREBASE_API_KEY` 외 5개 | 웹 Firebase 설정 — `.env.example` 참고 |
   | `GOOGLE_SERVICES_JSON` | 안드로이드 구글 로그인. 파일 내용 그대로 |
   | `ANDROID_KEYSTORE_BASE64` | 업로드 키. `base64 -w0 release.keystore` |
   | `ANDROID_KEY_ALIAS`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_PASSWORD` | 그 키의 별칭과 비밀번호 |

4. **릴리스는 자동입니다.** 기본 브랜치에 푸시하면 워크플로가 판올림된 새 버전으로
   태그를 만들고, APK를 붙인 릴리스를 냅니다. 앱의 업데이트 버튼이 그걸 봅니다.

   | 무엇을 하면 | 무엇이 나오는지 |
   | --- | --- |
   | 기본 브랜치에 푸시 | 패치 판올림 (`v1.0.1` → `v1.0.2`) + 릴리스 |
   | 그 밖의 브랜치에 푸시 | `ci` 프리릴리스 — 시험용. 업데이트 버튼은 못 봅니다 |
   | Actions → Run workflow (`bump`) | 고른 폭만큼 판올림 + 릴리스 |
   | 태그 `v1.2.3`을 직접 푸시 | 그 이름 그대로 릴리스 |

   다음 버전은 저장소에 있는 가장 큰 `v*` 태그에서 계산합니다 — 버전을 적어 두는
   파일이 없으므로 올리는 걸 깜빡할 자리도 없습니다. 문서만 고친 커밋처럼 판을
   올리고 싶지 않을 때는 커밋 메시지에 `[skip release]`를 넣으면 빠집니다.

처음 설치할 때 안드로이드가 "출처를 알 수 없는 앱" 허용을 한 번 묻습니다.

### 업데이트가 어떻게 도착하는지

안드로이드는 사이드로딩한 앱이 APK를 소리 없이 설치하도록 두지 않습니다. 설치 확인
화면은 시스템이 띄우는 것이고 어떤 라이브러리로도 없앨 수 없습니다. 그런데 이 앱에서
실제로 자주 바뀌는 것은 APK가 아니라 그 안의 웹 자산입니다 — 점수판 배치도, 기록
화면도, 규칙 계산도 전부 WebView가 읽는 파일입니다. 그건 갈아끼울 수 있습니다.

그래서 릴리스마다 세 개가 올라갑니다:

| 자산 | 쓰임 |
| --- | --- |
| `billiard-<버전>.apk` | 껍데기. 네이티브가 바뀐 버전에서만 필요합니다 |
| `www-<버전>.zip` | 웹 자산. 앱이 조용히 받아 다음 실행에 적용합니다 |
| `native-<지문>.txt` | 껍데기의 지문. 앱이 APK가 필요한지 판단하는 근거입니다 |

앱은 켜질 때 최신 릴리스를 보고, 웹 번들이 있으면 받아서 *다음 실행 자리*에 놓습니다.
받자마자 갈아끼우지 않는 이유는 치던 게임 도중에 화면이 새로 뜨지 않게 하려는
것입니다. 껍데기 지문이 자기 것과 다를 때만 APK를 미리 받아 두고 배지를 켭니다.

새 번들이 앱을 띄우지 못하면 다음 실행에서 이전 번들로 되돌아갑니다 —
`notifyAppReady()`를 받지 못한 번들은 실패한 것으로 봅니다. 그래서 잘못된 번들을
올려도 사용자의 앱이 못 켜지는 상태로 남지는 않습니다.

---

## 코드가 어디 있는지

| 파일 | 무엇 |
| --- | --- |
| `lib/game.ts` | 게임 규칙 전부 — 점수, 이닝, 되돌리기, 쿠션, 승패. UI를 모릅니다 |
| `lib/stats.ts` | 통계 계산. 오늘·이번 달 경계는 기기의 지역 시간으로 자릅니다 |
| `lib/storage.ts` | 기기 저장(진행 중인 게임, 기록, 설정) |
| `lib/firebase.ts` | 로그인과 클라우드 사본. 설정이 없으면 전부 조용히 꺼집니다 |
| `lib/update.ts` | 릴리스 확인과 APK 설치 |
| `lib/platform.ts` | 저장소·진동·화면 켜 두기·상태 표시줄 — 웹과 셸 양쪽 |
| `app/lobby.tsx` | 이름 → 종목 → 핸디 → 시작 |
| `app/game/scoreboard.tsx` | 점수판 |
| `app/stats/stats-view.tsx` | 통계 화면 |
| `app/settings/settings-panel.tsx` | 저장 방식, 계정, 기본값, 업데이트 |

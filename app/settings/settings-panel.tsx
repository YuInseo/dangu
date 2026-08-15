'use client';

import { useEffect, useState } from 'react';

import { clipboard, getPlatform, tap } from '../../lib/platform';
import {
  firebaseConfig,
  loadFirebaseConfig,
  parseFirebaseSnippet,
  saveFirebaseConfig,
  type FirebaseConfig,
} from '../../lib/firebase';
import {
  DEFAULT_SETTINGS,
  loadSettings,
  saveSettings,
  watchSettings,
  type AppSettings,
} from '../../lib/storage';
import {
  checkForUpdate,
  currentVersion,
  installUpdate,
  needsApk,
  platformLine,
  type InstallProgress,
  type UpdateCheck,
} from '../../lib/update';
import {
  applyNow,
  diagnose,
  stageWebBundle,
  type StageResult,
  type UpdateDiagnosis,
} from '../../lib/live-update';
import { useAccount } from '../../lib/use-account';
import { SignIn } from '../sign-in';

/**
 * 설정과 업데이트.
 *
 * 업데이트가 설정 안에 있는 이유: 스토어 없이 배포하는 앱이라 새 버전을 앱이 스스로
 * 가져와야 하고, 그건 사용자가 "내 앱"에 대해 묻는 다른 질문들과 같은 자리에 있는 게 맞다.
 * 상단 줄의 업데이트 버튼도 결국 이 화면의 이 카드로 온다.
 */
export function SettingsPanel() {
  const account = useAccount();
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  /** 지금 적고 있는 당구장 이름. */
  const [adding, setAdding] = useState('');

  /*
   * 설정은 이제 이 화면 바깥에서도 바뀐다.
   *
   * 태블릿에서 당구장을 하나 더하면 그 값이 이 폰으로 밀려 오고, 로그인하면 저장 방식이
   * 계정으로 옮겨진다. 한 번 읽고 마는 화면은 그때부터 거짓말을 한다 — 방금 바뀐 값을
   * 옛 값으로 보여 주고, 거기서 무언가를 고치면 옛 값이 다시 저장된다.
   */
  useEffect(() => {
    let alive = true;
    void loadSettings().then((stored) => {
      if (alive) setSettings(stored);
    });
    const stop = watchSettings(setSettings);
    return () => {
      alive = false;
      stop();
    };
  }, []);

  const update = (next: Partial<AppSettings>) => {
    const merged = { ...settings, ...next };
    setSettings(merged);
    void saveSettings(merged);
  };

  return (
    <div className="page">
      <UpdateCard />

      <div className="card">
        <h2>저장 방식</h2>
        <p>
          기록을 어디에 둘지 고릅니다. 어느 쪽을 골라도 게임은 항상 이 기기에 먼저 저장됩니다 —
          당구장에 네트워크가 없어도 점수판은 끝까지 돌아가야 하니까요.
        </p>
        {/*
          누른 순간 `syncPinned`가 붙는다.

          이 값이 하는 일은 앱의 자동 판단을 막는 것이다 — 로그인한 사람의 기록은 묻지
          않고 계정으로 옮기는데, 그 규칙이 "이 기기에만"을 직접 고른 사람에게까지
          적용되면 그건 편의가 아니라 결정을 뒤집는 일이 된다. 기본값으로서의 `local`과
          사람이 고른 `local`은 다르고, 그 차이를 적어 두는 자리가 여기다.
        */}
        <div className="choices" style={{ gridTemplateColumns: '1fr 1fr' }}>
          <button
            className="choice"
            aria-pressed={settings.sync === 'local'}
            onClick={() => update({ sync: 'local', syncPinned: true })}
          >
            이 기기에만
            <small>계정 없이. 폰을 바꾸면 기록은 따라오지 않습니다.</small>
          </button>
          <button
            className="choice"
            aria-pressed={settings.sync === 'cloud'}
            onClick={() => {
              update({ sync: 'cloud', syncPinned: true });
              // 고른 그 자리에서 맞춘다. 켜 두고 다음에 앱을 켤 때까지 기다릴 이유가 없다.
              void account.syncNow();
            }}
          >
            구글 계정
            <small>기록도 설정도 계정에 둡니다. 다른 기기에서 바로 이어집니다.</small>
          </button>
        </div>

        {settings.sync === 'local' && settings.syncPinned && account.account && (
          <p style={{ fontSize: '0.82rem' }}>
            로그인해 있지만 이 기기에만 두기로 골랐습니다. 계정에는 아무것도 올라가지
            않습니다 — 위에서 “구글 계정”을 고르면 그때부터 올라갑니다.
          </p>
        )}

        {settings.sync === 'cloud' && account.configured === false && (
          <p className="notice warn">
            Firebase 설정이 없어 클라우드 저장이 실제로는 동작하지 않습니다. 아래 "Firebase 설정"에
            콘솔의 설정을 붙여넣으면 켜집니다.
          </p>
        )}
        {settings.sync === 'cloud' && account.configured === true && !account.account && (
          <p className="notice warn">아래에서 로그인해야 클라우드에 올라갑니다.</p>
        )}
      </div>

      <FirebaseCard />

      <div className="card">
        <h2>계정</h2>
        {account.configured === false && (
          <p>
            Firebase가 설정되지 않았습니다. 기록은 이 기기에만 저장됩니다 — 앱의 모든 기능은 그대로
            동작합니다.
          </p>
        )}

        {account.configured === true && !account.account && <SignIn account={account} />}

        {account.account && (
          <>
            <p>
              {account.account.name}
              {account.account.email ? ` · ${account.account.email}` : ''}
            </p>
            {/*
              맞추기는 자동이다. 여기 있는 것은 버튼이 아니라 그 결과다.

              올리기·받기 버튼이 있던 자리인데, 그 둘을 언제 눌러야 하는지 아는 사람은
              만든 사람뿐이었다. 폰을 바꿨을 때, 다른 기기에서 쳤을 때, 지하에서 안
              올라갔을 때 — 전부 앱이 알 수 있는 일이다.
            */}
            {settings.sync === 'cloud' ? (
              <>
                <p className={account.sync.state === 'error' ? 'notice warn' : 'notice'}>
                  {account.sync.state === 'syncing' && '기록을 맞추는 중…'}
                  {account.sync.state === 'done' &&
                    (account.sync.up + account.sync.down === 0
                      ? '기록이 계정과 같습니다.'
                      : `맞췄습니다 — ${account.sync.down}건 받고 ${account.sync.up}건 올렸습니다.`)}
                  {account.sync.state === 'error' &&
                    `맞추지 못했습니다. ${account.sync.reason ?? ''}`}
                  {account.sync.state === 'idle' && '앱을 켤 때마다 자동으로 맞춥니다.'}
                </p>
                {/*
                  앱이 스스로 저장 위치를 옮겼으면 그 사실을 말한다.

                  설명 없이 바뀐 설정은 편의가 아니라 놀랄 일이다. 되돌리는 길이 바로
                  위에 있다는 것도 같이 적는다 — 앱이 한 판단은 사람이 뒤집을 수 있어야
                  한다.
                */}
                {account.sync.migrated && (
                  <p className="notice">
                    로그인해서 저장 방식을 <strong>구글 계정</strong>으로 옮겼습니다. 이 기기의
                    기록과 설정이 계정으로 올라가고, 다른 기기에서 친 판도 여기로 내려옵니다.
                    원하지 않으면 위에서 “이 기기에만”을 고르세요.
                  </p>
                )}
                <p style={{ fontSize: '0.78rem' }}>
                  기록, 지운 기록, 그리고 내 이름·당구장·종목 같은 설정이 함께 맞춰집니다.
                  진동과 화면 켜 두기는 기기마다 달라서 따라가지 않습니다.
                </p>
              </>
            ) : (
              <p style={{ fontSize: '0.82rem' }}>
                저장 방식이 “이 기기에만”이라 계정에는 올라가지 않습니다. 위에서 “구글 계정”을
                고르면 그때부터 자동으로 맞춥니다.
              </p>
            )}

            {settings.sync === 'cloud' && (
              <button
                className="secondary"
                disabled={account.sync.state === 'syncing'}
                onClick={() => void account.syncNow()}
              >
                {account.sync.state === 'syncing' ? '맞추는 중…' : '지금 맞추기'}
              </button>
            )}
            <button className="ghost" onClick={() => void account.signOutNow()}>
              로그아웃
            </button>
          </>
        )}
      </div>

      <div className="card">
        <h2>기본값</h2>
        <div>
          <label className="label" htmlFor="myName">
            내 이름
          </label>
          <input
            id="myName"
            value={settings.myName}
            onChange={(event) => update({ myName: event.target.value })}
          />
        </div>

        <label className="switch-row">
          <span>점수 버튼 진동</span>
          <input
            type="checkbox"
            checked={settings.haptics}
            onChange={(event) => update({ haptics: event.target.checked })}
          />
        </label>

        <label className="switch-row">
          <span>게임 중 화면 켜 두기</span>
          <input
            type="checkbox"
            checked={settings.keepAwake}
            onChange={(event) => update({ keepAwake: event.target.checked })}
          />
        </label>

        <label className="switch-row">
          <span>남은 점수 소리로 알리기</span>
          <input
            type="checkbox"
            checked={settings.voice !== false}
            onChange={(event) => update({ voice: event.target.checked })}
          />
        </label>
        <p style={{ fontSize: '0.78rem' }}>
          점수를 올릴 때마다 “17점 남았습니다”처럼 읽어 줍니다. 큐를 들고 있으면 화면을
          볼 수 없어서 붙인 기능이고, 기기에 깔린 한국어 음성을 그대로 씁니다 — 음성이 없는
          기기에서는 켜도 소리가 나지 않습니다.
        </p>

        {/*
          샷 클락.
          3쿠션 공식이 40초라 그걸 가운데 두었다. 기본은 끔 — 동호회에서 치는 사람에게
          시간 제한은 있으면 재미있는 것이지 없으면 안 되는 것이 아니다.
        */}
        <span className="label">한 차례에 주는 시간</span>
        <div className="choices">
          {[0, 30, 40, 60].map((seconds) => (
            <button
              key={seconds}
              className="choice"
              aria-pressed={(settings.turnSeconds ?? 0) === seconds}
              onClick={() => update({ turnSeconds: seconds })}
            >
              {seconds === 0 ? '끄기' : `${seconds}초`}
            </button>
          ))}
        </div>
        <p style={{ fontSize: '0.78rem' }}>
          켜면 점수판 아래에 남은 시간이 막대로 흐릅니다. 초록에서 노랑을 지나 빨강이 되고,
          다 쓰면 한 번 울립니다. 일시정지하면 같이 멈춥니다.
        </p>
      </div>

      {/*
        다니는 당구장.

        기록에서 되짚을 수도 있지만 그건 "친 적 있는 집"이고, 여기 적는 것은 "다니는
        집"이다. 둘은 다르다 — 새로 생긴 집은 아직 기록이 없어도 오늘 갈 수 있고, 상단
        줄의 고르개는 그 집을 알아야 한다.
      */}
      <div className="card">
        <h2>당구장</h2>
        <p>
          자주 가는 곳을 적어 두면 첫 화면 위쪽에서 골라 둘 수 있고, 그날 친 판에 장소가
          함께 남습니다.
        </p>

        <form
          className="add-place"
          onSubmit={(event) => {
            event.preventDefault();
            const name = adding.trim();
            if (!name) return;
            const list = settings.venues ?? [];
            if (!list.includes(name)) update({ venues: [...list, name] });
            setAdding('');
          }}
        >
          <input
            value={adding}
            placeholder="예: 대박당구장"
            aria-label="당구장 이름"
            onChange={(event) => setAdding(event.target.value)}
          />
          <button type="submit" className="secondary">
            추가
          </button>
        </form>

        {(settings.venues ?? []).length === 0 ? (
          <p style={{ fontSize: '0.82rem' }}>아직 적어 둔 곳이 없습니다.</p>
        ) : (
          <div className="places">
            {(settings.venues ?? []).map((name) => {
              const home = settings.lastVenue === name;
              return (
                <div className="place" key={name}>
                  <span>{name}</span>
                  {/*
                    기본으로 삼기.

                    고른 집은 첫 화면 위쪽에 떠 있고, 새 판을 차릴 때 장소 칸에 미리
                    채워진다. 늘 같은 집에 가는 사람은 이걸 한 번 눌러 두면 장소를
                    다시는 고르지 않는다. 이미 기본인 것을 누르면 풀린다 — 켜는 길만
                    있고 끄는 길이 없으면 그건 고르개가 아니다.
                  */}
                  <button
                    className={home ? 'choice on' : 'choice'}
                    aria-pressed={home}
                    onClick={() => {
                      update({ lastVenue: home ? '' : name });
                      tap();
                    }}
                  >
                    기본
                  </button>
                  <button
                    className="ghost"
                    aria-label={`${name} 지우기`}
                    onClick={() =>
                      update({
                        venues: (settings.venues ?? []).filter((entry) => entry !== name),
                        // 기본으로 삼았던 집을 지우면 기본도 함께 사라진다.
                        ...(home ? { lastVenue: '' } : {}),
                      })
                    }
                  >
                    지우기
                  </button>
                </div>
              );
            })}
          </div>
        )}
        <p style={{ fontSize: '0.78rem' }}>
          여기서 지워도 이미 친 기록의 장소는 그대로 남습니다 — 지난 일을 고치는 자리가
          아니라, 앞으로 고를 목록을 정하는 자리입니다.
        </p>
      </div>

      <div className="card">
        <h2>앱 정보</h2>
        <p>
          {platformLine()} · 실행 환경 <code>{getPlatform()}</code>
        </p>
      </div>
    </div>
  );
}

/* Firebase 설정 ------------------------------------------------------ */

/**
 * Firebase 설정을 앱 안에서 직접 넣는 자리.
 *
 * 사이드로딩으로 배포하는 앱에 "환경변수를 넣고 다시 빌드하세요"는 할 수 없는 요구다.
 * 그래서 콘솔에서 복사한 설정 덩어리를 그대로 붙여넣게 하고, 필요한 여섯 값을 여기서
 * 꺼낸다 — 폰에서 여섯 칸을 따로 채우는 것보다 실수가 적고, JSON이든 자바스크립트
 * 조각이든 상관없다.
 *
 * 이 값들은 원래 공개되는 값이라 기기에 평문으로 두어도 잃을 것이 없다. 실제 보호는
 * Firestore 규칙이 한다 — 저장소의 `firestore.rules`.
 */
const PLACEHOLDER = [
  'const firebaseConfig = {',
  '  apiKey: "AIza…",',
  '  authDomain: "my-app.firebaseapp.com",',
  '  projectId: "my-app",',
  '  appId: "1:123…:web:abc…"',
  '};',
].join('\n');

function FirebaseCard() {
  const [config, setConfig] = useState<FirebaseConfig | null>(null);
  const [text, setText] = useState('');
  const [message, setMessage] = useState<string | null>(null);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    void loadFirebaseConfig().then(setConfig);
  }, []);

  const fromBuild = firebaseConfig();
  const source = config === null ? '없음' : config.apiKey === fromBuild?.apiKey ? '빌드에 포함된 값' : '이 기기에 입력한 값';

  const apply = async (raw: string) => {
    const parsed = parseFirebaseSnippet(raw);
    if (!parsed) {
      setMessage('apiKey · authDomain · projectId · appId를 찾지 못했습니다. 콘솔의 설정을 통째로 붙여넣어 보세요.');
      return;
    }
    await saveFirebaseConfig(parsed);
    setConfig(parsed);
    setText('');
    setMessage(`저장했습니다 — ${parsed.projectId}. 이제 로그인할 수 있습니다.`);
  };

  return (
    <div className="card" id="firebase">
      <h2>Firebase 설정</h2>
      <p>
        현재: <strong>{config ? config.projectId : '설정 없음'}</strong> ({source})
      </p>

      {!open && (
        <button className="secondary" onClick={() => setOpen(true)}>
          {config ? '설정 바꾸기' : '설정 입력하기'}
        </button>
      )}

      {open && (
        <>
          <p>
            Firebase 콘솔 → 프로젝트 설정 → 내 앱 → 웹 앱의 <code>firebaseConfig</code>를 통째로
            복사해 붙여넣으세요. JSON이든 코드 조각이든 됩니다.
          </p>
          <textarea
            value={text}
            onChange={(event) => setText(event.target.value)}
            rows={7}
            spellCheck={false}
            placeholder={PLACEHOLDER}
            style={{
              width: '100%',
              minHeight: '9rem',
              borderRadius: 10,
              border: '1px solid rgba(255,255,255,0.14)',
              background: '#0f1115',
              color: 'var(--center-ink)',
              padding: '0.7rem',
              fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
              fontSize: '0.8rem',
              lineHeight: 1.45,
            }}
          />

          <div className="row">
            <button
              className="secondary"
              onClick={async () => {
                // 폰에서는 붙여넣기가 길게 누르기라, 버튼 하나로 끝내는 편이 낫다.
                const read = await clipboard.readText({ prompt: true, timeoutMs: 20000 });
                if (read.supported && read.text) setText(read.text);
                else setMessage('클립보드를 읽지 못했습니다. 입력칸을 길게 눌러 붙여넣으세요.');
              }}
            >
              클립보드에서
            </button>
            <button className="primary" onClick={() => void apply(text)} disabled={!text.trim()}>
              저장
            </button>
          </div>

          <button className="ghost" onClick={() => setOpen(false)}>
            닫기
          </button>

          {config && (
            <button
              className="danger"
              onClick={async () => {
                await saveFirebaseConfig(null);
                setConfig(firebaseConfig());
                setMessage('이 기기에 입력한 설정을 지웠습니다.');
              }}
            >
              입력한 설정 지우기
            </button>
          )}
        </>
      )}

      {message && <p className="notice">{message}</p>}
    </div>
  );
}

/* 업데이트 ----------------------------------------------------------- */

/**
 * 업데이트 카드.
 *
 * 여기서 사용자가 할 일은 대개 없다. 웹 번들은 앱이 켜질 때 이미 받아 두었고 다음
 * 실행에 적용된다 — 이 카드는 그 사실을 알려 줄 뿐이다. 버튼이 필요한 경우는 하나,
 * 앱 껍데기까지 바뀌어서 안드로이드 설치 화면을 거쳐야 할 때다.
 */
function UpdateCard() {
  const [check, setCheck] = useState<UpdateCheck | null>(null);
  const [stage, setStage] = useState<StageResult | null>(null);
  const [apk, setApk] = useState(false);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<InstallProgress | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  /** 이 앱이 업데이트에 대해 아는 것 전부. 안 될 때 볼 수 있어야 고칠 수 있다. */
  const [info, setInfo] = useState<UpdateDiagnosis | null>(null);
  const [showInfo, setShowInfo] = useState(false);

  /** `force`는 사람이 "다시 확인"을 눌렀다는 뜻이다 — 그때만 캐시를 건너뛴다. */
  const run = async (force = false) => {
    setBusy(true);
    setMessage(null);
    const result = await checkForUpdate(force);
    setCheck(result);
    if (result.state === 'available') {
      setStage(await stageWebBundle(result.release));
      setApk(await needsApk(result.release));
    }
    setInfo(await diagnose());
    setBusy(false);
  };

  useEffect(() => {
    void run();
  }, []);

  const available = check?.state === 'available' ? check : null;

  return (
    <div className="card" id="update">
      <h2>업데이트</h2>

      {check?.state === 'unconfigured' && (
        <p>
          릴리스를 확인할 GitHub 저장소가 설정되지 않았습니다. <code>GRAFT_PUBLIC_GITHUB_REPO</code>를
          <code>owner/repo</code> 형태로 설정하고 다시 빌드하세요.
        </p>
      )}

      {check?.state === 'current' && <p>최신 버전입니다 (v{check.version}).</p>}

      {check?.state === 'error' && <p className="notice warn">{check.reason}</p>}

      {available && (
        <>
          <p>
            새 버전 <strong>v{available.version}</strong>. 현재 v{currentVersion()}.
          </p>
          {available.release.notes && (
            <p className="notice" style={{ whiteSpace: 'pre-wrap' }}>
              {available.release.notes.slice(0, 400)}
            </p>
          )}

          {/*
            받아 둔 뒤의 자리.

            예전에는 여기서 "다시 켜면 적용됩니다"만 말하고 끝이었다. 그런데 다시 켜도
            안 바뀌는 날이 있었고, 그때 사용자가 할 수 있는 일이 하나도 없었다 — 무엇이
            잘못됐는지 화면에 없었기 때문이다. 그래서 둘을 더했다: 지금 바로 적용하는
            버튼과, 안 됐을 때 그 이유.
          */}
          {stage?.state === 'ready' && (
            <>
              <p className="notice">
                <strong>v{stage.version}을 받아 두었습니다.</strong>
                <br />
                앱을 완전히 닫았다가 다시 켜면 적용됩니다. 기다리지 않으려면 아래 버튼을
                누르세요 — 지금 갈아끼우고 화면을 새로 엽니다.
              </p>
              <button
                className="primary"
                disabled={busy}
                onClick={async () => {
                  setBusy(true);
                  setMessage(null);
                  const result = await applyNow(available.release);
                  setBusy(false);
                  if (result.state !== 'ready') {
                    setMessage(
                      result.state === 'error' ? result.reason : '지금은 적용할 수 없습니다.'
                    );
                  }
                }}
              >
                v{stage.version} 지금 적용
              </button>
            </>
          )}

          {stage?.state === 'error' && (
            <p className="notice warn">
              <strong>받지 못했습니다.</strong>
              <br />
              {stage.reason}
            </p>
          )}

          {/* 브라우저에는 갈아끼울 껍데기가 없다 — 그건 고장이 아니라 그냥 웹이다. */}
          {stage?.state === 'unsupported' && getPlatform() !== 'web' && (
            <p className="notice warn">
              이 앱 껍데기는 화면만 갈아끼우는 방식을 쓰지 못합니다. 아래에서 APK를 한 번
              설치하면 그다음부터는 조용히 업데이트됩니다.
            </p>
          )}

          {apk && (
            <>
              <p style={{ fontSize: '0.82rem' }}>
                이번 버전은 앱 껍데기까지 바뀌어서 설치가 한 번 필요합니다. 파일은 이미 받아
                두었으므로 누르면 곧바로 설치 화면입니다.
              </p>
              <button
                className="primary"
                disabled={busy}
                onClick={async () => {
                  setBusy(true);
                  setMessage(null);
                  const result = await installUpdate(available.release, setProgress);
                  setBusy(false);
                  setProgress(null);
                  setMessage(
                    result.ok
                      ? result.via === 'installer'
                        ? '설치 화면이 열립니다. "설치"를 누르면 끝납니다.'
                        : '브라우저에서 내려받는 중입니다. 알림을 눌러 설치하세요.'
                      : result.reason
                  );
                }}
              >
                {busy && progress
                  ? `받는 중 ${Math.round((progress.downloaded / Math.max(1, progress.total)) * 100)}%`
                  : `v${available.version} 설치`}
              </button>
            </>
          )}
        </>
      )}

      {/*
        어떤 상태에서도 APK로 가는 길은 열어 둔다.

        조용한 업데이트가 막힌 이유를 앱이 늘 알 수 있는 것은 아니다 — 플러그인이
        없을 수도, 받다가 실패했을 수도, GitHub이 한도를 걸었을 수도 있다. 그 모든
        경우에 공통으로 통하는 길이 하나 있으므로, 판단이 서지 않아도 그 길만은
        가려 두지 않는다.
      */}
      {available?.release.apk && !apk && (
        <a
          className="ghost"
          href={available.release.apk.url}
          target="_blank"
          rel="noreferrer"
          style={{ display: 'block', textAlign: 'center' }}
        >
          APK 직접 받기 (v{available.version})
        </a>
      )}

      <button className="ghost" onClick={() => void run(true)} disabled={busy}>
        {busy ? '확인 중…' : '다시 확인'}
      </button>

      {message && <p className="notice">{message}</p>}

      <p style={{ fontSize: '0.78rem' }}>
        새 버전은 앱이 켜질 때 알아서 받아 두고 다음 실행에 적용됩니다. 앱 껍데기가 바뀔 때만
        안드로이드 설치 화면이 한 번 뜨는데, 그건 시스템이 요구하는 것이라 건너뛸 수 없습니다.
      </p>

      {/* 잘 될 때는 접혀 있고, 안 될 때만 펼쳐 본다. */}
      <button className="ghost" onClick={() => setShowInfo((open) => !open)}>
        {showInfo ? '업데이트 상태 접기' : '업데이트 상태 보기'}
      </button>
      {showInfo && (
        <p className="notice" style={{ fontSize: '0.74rem', whiteSpace: 'pre-wrap' }}>
          {[
            `화면 v${currentVersion()}`,
            `업데이트 기능 ${info?.plugin ? '있음' : '없음'}`,
            `껍데기 v${info?.native ?? '?'}`,
            `적용된 번들 ${info?.bundle ?? '기본'}`,
            `다음 번들 ${info?.next ?? '없음'}`,
            `받아 둔 것 ${
              info?.bundles.length
                ? info.bundles.map((entry) => `${entry.version}(${entry.status})`).join(', ')
                : '없음'
            }`,
          ].join('\n')}
        </p>
      )}
    </div>
  );
}

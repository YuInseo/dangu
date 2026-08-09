'use client';

import { useEffect, useState } from 'react';

import { clipboard, getPlatform } from '../../lib/platform';
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
import { stageWebBundle, type StageResult } from '../../lib/live-update';
import { syncDown, syncUp, useAccount } from '../../lib/use-account';

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
  const [syncing, setSyncing] = useState<string | null>(null);

  useEffect(() => {
    void loadSettings().then(setSettings);
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
        <div className="choices" style={{ gridTemplateColumns: '1fr 1fr' }}>
          <button
            className="choice"
            aria-pressed={settings.sync === 'local'}
            onClick={() => update({ sync: 'local' })}
          >
            이 기기에만
            <small>계정 없이. 폰을 바꾸면 기록은 따라오지 않습니다.</small>
          </button>
          <button
            className="choice"
            aria-pressed={settings.sync === 'cloud'}
            onClick={() => update({ sync: 'cloud' })}
          >
            구글 계정
            <small>Firebase에 사본을 둡니다. 폰을 바꿔도 남습니다.</small>
          </button>
        </div>

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

        {account.configured === true && !account.account && (
          <>
            <p>구글로 로그인하면 기록이 계정에 저장되어 폰을 바꿔도 남습니다.</p>
            <button className="secondary" onClick={() => void account.signIn()} disabled={account.signingIn}>
              {account.signingIn ? '로그인 중…' : 'Google로 로그인'}
            </button>
            {account.error && <p className="notice error">{account.error}</p>}
          </>
        )}

        {account.account && (
          <>
            <p>
              {account.account.name}
              {account.account.email ? ` · ${account.account.email}` : ''}
            </p>
            <div className="row">
              <button
                className="secondary"
                onClick={async () => {
                  setSyncing('올리는 중…');
                  const count = await syncUp(account.account!.uid);
                  setSyncing(`${count}건 올렸습니다.`);
                }}
              >
                기록 올리기
              </button>
              <button
                className="secondary"
                onClick={async () => {
                  setSyncing('받는 중…');
                  const list = await syncDown(account.account!.uid);
                  setSyncing(`${list.length}건이 이 기기에 있습니다.`);
                }}
              >
                기록 받기
              </button>
            </div>
            {syncing && <p className="notice">{syncing}</p>}
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

        <label className="row" style={{ justifyContent: 'space-between' }}>
          <span>점수 버튼 진동</span>
          <input
            type="checkbox"
            checked={settings.haptics}
            style={{ width: '3rem', minHeight: 0 }}
            onChange={(event) => update({ haptics: event.target.checked })}
          />
        </label>

        <label className="row" style={{ justifyContent: 'space-between' }}>
          <span>게임 중 화면 켜 두기</span>
          <input
            type="checkbox"
            checked={settings.keepAwake}
            style={{ width: '3rem', minHeight: 0 }}
            onChange={(event) => update({ keepAwake: event.target.checked })}
          />
        </label>

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

  const run = async () => {
    setBusy(true);
    setMessage(null);
    const result = await checkForUpdate();
    setCheck(result);
    if (result.state === 'available') {
      setStage(await stageWebBundle(result.release));
      setApk(await needsApk(result.release));
    }
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

          {stage?.state === 'ready' && (
            <p className="notice">
              <strong>v{stage.version}을 받아 두었습니다.</strong>
              <br />
              앱을 완전히 닫았다가 다시 켜면 적용됩니다 — 화면만 나갔다 오는 것으로는 바뀌지
              않습니다.
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

      <button className="ghost" onClick={() => void run()} disabled={busy}>
        {busy ? '확인 중…' : '다시 확인'}
      </button>

      {message && <p className="notice">{message}</p>}

      <p style={{ fontSize: '0.78rem' }}>
        새 버전은 앱이 켜질 때 알아서 받아 두고 다음 실행에 적용됩니다. 앱 껍데기가 바뀔 때만
        안드로이드 설치 화면이 한 번 뜨는데, 그건 시스템이 요구하는 것이라 건너뛸 수 없습니다.
      </p>
    </div>
  );
}

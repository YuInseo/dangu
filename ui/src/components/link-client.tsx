/**
 * 설정 · 연결. 이 기기는 혼자서도 다 된다. 원할 때만 같은 와이파이의 다른 기기에 열거나,
 * PC·서버의 소리에 연결한다.
 */
import { useEffect, useState } from 'react';
import { useApi } from '@/api';
import { Switch } from '@/components/dsp/ui';
import { KeyRound, Laptop, LoaderCircle, Lock, RefreshCw, Server, Trash2, Wifi } from '@/components/icons';
import { useToast } from '@/components/toast';
import { type Found, type LinkInfo, hello, link, normalizeUrl, openRemote } from '@/lib/link';

/** 이 기기를 같은 와이파이의 다른 기기에 연다. */
export function ShareCard() {
  const toast = useToast();
  const { data } = useApi<LinkInfo>('/api/link');
  const [info, setInfo] = useState<LinkInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const cur = info ?? data ?? null;
  if (!cur) return <div className="pref-row muted">불러오는 중…</div>;

  const run = async (f: () => Promise<LinkInfo>) => {
    setBusy(true);
    try {
      setInfo(await f());
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), 'error');
    } finally {
      setBusy(false);
    }
  };

  if (cur.mode === 'always') {
    return (
      <div className="link-card">
        <div className="pref-row">
          <div>
            <div className="pref-label">이 서버는 처음부터 네트워크에 열려 있습니다</div>
            <div className="pref-hint">
              다른 기기에서 아래 주소를 엽니다.{cur.password ? ' 정해 둔 비밀번호로 들어옵니다.' : ' 비밀번호가 없으니 같은 망의 누구나 열 수 있습니다(SORI_PASSWORD).'}
            </div>
          </div>
        </div>
        <Addresses urls={cur.urls} />
      </div>
    );
  }

  return (
    <div className="link-card">
      <div className="pref-row">
        <div>
          <div className="pref-label">같은 와이파이의 다른 기기에서 열기</div>
          <div className="pref-hint">
            기본은 꺼짐: 이 기기 안에서만 열립니다. 켜면 휴대폰이나 다른 PC가 이 기기의 라이브러리를 듣고, 이 기기의 스피커로 틀 수
            있습니다. PIN으로 잠기고, 끄면 열려 있던 연결도 바로 끊깁니다.
          </div>
        </div>
        <div className="pref-control">
          {cur.local ? (
            <Switch label="같은 와이파이에 열기" on={cur.mode === 'on'} disabled={busy} onChange={(v) => void run(() => link.setLan(v))} />
          ) : (
            <span className="muted">그 기기에서만 바꿀 수 있습니다</span>
          )}
        </div>
      </div>
      {cur.local && <NameRow name={cur.name} onSaved={setInfo} />}
      {cur.mode === 'on' && (
        <>
          <Addresses urls={cur.urls} />
          {cur.pin ? (
            <div className="pref-row">
              <div>
                <div className="pref-label">PIN</div>
                <div className="pref-hint">연결하는 기기에서 처음 한 번 넣습니다. 바꾸면 연결해 둔 기기는 다시 넣어야 합니다.</div>
              </div>
              <div className="pref-control link-pin-wrap">
                <span className="link-pin" aria-label={`PIN ${cur.pin.split('').join(' ')}`}>
                  <KeyRound size={18} />
                  {cur.pin.slice(0, 3)} {cur.pin.slice(3)}
                </span>
                <button type="button" className="btn btn-sm" disabled={busy} onClick={() => void run(link.renewPin)}>
                  바꾸기
                </button>
              </div>
            </div>
          ) : (
            cur.password && <div className="link-empty">정해 둔 비밀번호로 들어옵니다.</div>
          )}
        </>
      )}
    </div>
  );
}

/** 다른 기기에서 찾을 때 보이는 이름 */
function NameRow({ name, onSaved }: { name: string; onSaved: (i: LinkInfo) => void }) {
  const toast = useToast();
  const [value, setValue] = useState(name);
  const [saving, setSaving] = useState(false);
  const changed = value.trim() !== name;
  return (
    <form
      className="pref-row"
      onSubmit={async (e) => {
        e.preventDefault();
        setSaving(true);
        try {
          const i = await link.rename(value);
          onSaved(i);
          setValue(i.name);
          toast('이름을 바꿨습니다');
        } catch (err) {
          toast(err instanceof Error ? err.message : String(err), 'error');
        } finally {
          setSaving(false);
        }
      }}
    >
      <div>
        <div className="pref-label">이 기기의 이름</div>
        <div className="pref-hint">다른 기기에서 찾을 때 보이는 이름입니다. 예: 거실 PC</div>
      </div>
      <div className="pref-control link-name">
        <label className="filter">
          <input value={value} maxLength={40} onChange={(e) => setValue(e.target.value)} aria-label="이 기기의 이름" />
        </label>
        {changed && (
          <button type="submit" className="btn btn-sm" disabled={saving}>
            저장
          </button>
        )}
      </div>
    </form>
  );
}

function Addresses({ urls }: { urls: string[] }) {
  if (!urls.length) {
    return <div className="link-empty">와이파이(LAN) 주소를 찾지 못했습니다. 와이파이에 연결되어 있는지 보세요.</div>;
  }
  return (
    <div className="pref-row pref-row-stack">
      <div className="pref-label">다른 기기에서 열 주소</div>
      <div className="link-urls">
        {urls.map((u) => (
          <code key={u} className="link-url">
            {u}
          </code>
        ))}
      </div>
      <div className="pref-hint">다른 기기의 소리 앱에서는 설정 · 연결의 「찾기」로도 나옵니다.</div>
    </div>
  );
}

/** PC나 서버의 소리에 연결한다. */
export function ConnectCard() {
  const toast = useToast();
  const { data } = useApi<LinkInfo>('/api/link');
  const [info, setInfo] = useState<LinkInfo | null>(null);
  const [found, setFound] = useState<Found[] | null>(null);
  const [searching, setSearching] = useState(false);
  const [addr, setAddr] = useState('');
  const [checking, setChecking] = useState(false);
  const cur = info ?? data ?? null;

  const search = async () => {
    setSearching(true);
    try {
      setFound(await link.discover());
    } catch {
      setFound([]);
    } finally {
      setSearching(false);
    }
  };

  // 처음 열면 한 번 찾아 본다.
  useEffect(() => {
    void search();
  }, []);

  const connect = async (raw: string, name?: string) => {
    const url = normalizeUrl(raw);
    if (!url) {
      toast('주소가 이상합니다. 예: 192.168.0.10:4390', 'error');
      return;
    }
    setChecking(true);
    try {
      const h = await hello(url);
      try {
        setInfo(await link.save({ name: name ?? h.name, url }));
      } catch {
        // 기억하지 못해도 연결은 한다
      }
      openRemote(url);
    } catch {
      toast(`그 주소에서 소리를 찾지 못했습니다: ${url}`, 'error');
      setChecking(false);
    }
  };

  const forget = async (url: string) => {
    try {
      setInfo(await link.forget(url));
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), 'error');
    }
  };

  return (
    <div className="link-card">
      <div className="pref-row">
        <div>
          <div className="pref-label">같은 와이파이의 소리</div>
          <div className="pref-hint">PC나 다른 기기에서 「같은 와이파이에 열기」를 켜 두면 여기에 나옵니다.</div>
        </div>
        <div className="pref-control">
          <button type="button" className="btn btn-sm" onClick={() => void search()} disabled={searching}>
            {searching ? <LoaderCircle size={15} className="spin" /> : <RefreshCw size={15} />}
            찾기
          </button>
        </div>
      </div>
      {found && found.length === 0 && !searching && <div className="link-empty">찾은 소리가 없습니다.</div>}
      {found && found.length > 0 && (
        <ul className="link-list">
          {found.map((f) => (
            <li key={f.id} className="link-item">
              <Laptop size={20} className="link-item-icon" />
              <div className="link-item-text">
                <div className="link-item-name">
                  {f.name}
                  {f.locked && <Lock size={13} className="muted" aria-label="PIN 필요" />}
                </div>
                <div className="muted">
                  {f.url} · v{f.version}
                </div>
              </div>
              <button type="button" className="btn btn-sm btn-primary" disabled={checking} onClick={() => void connect(f.url, f.name)}>
                열기
              </button>
            </li>
          ))}
        </ul>
      )}

      <form
        className="pref-row pref-row-stack"
        onSubmit={(e) => {
          e.preventDefault();
          void connect(addr);
        }}
      >
        <div className="pref-label">주소로 연결</div>
        <div className="dir-form">
          <label className="filter">
            <Server size={16} />
            <input
              value={addr}
              onChange={(e) => setAddr(e.target.value)}
              placeholder="192.168.0.10:4390 또는 https://music.example.com"
              aria-label="소리 주소"
              inputMode="url"
              autoCapitalize="off"
              autoCorrect="off"
              spellCheck={false}
            />
          </label>
          <button type="submit" className="btn" disabled={checking || !addr.trim()}>
            {checking ? <LoaderCircle size={15} className="spin" /> : <Wifi size={15} />}
            연결
          </button>
        </div>
        <div className="pref-hint">집 밖의 서버(도커 등)도 됩니다. 연결해도 이 기기의 라이브러리와 설정은 그대로 남습니다.</div>
      </form>

      {cur && cur.saved.length > 0 && (
        <>
          <h3 className="pref-group">연결했던 소리</h3>
          <ul className="link-list">
            {cur.saved.map((s) => (
              <li key={s.url} className="link-item">
                <Server size={20} className="link-item-icon" />
                <div className="link-item-text">
                  <div className="link-item-name">{s.name}</div>
                  <div className="muted">{s.url}</div>
                </div>
                <button type="button" className="btn btn-sm btn-primary" disabled={checking} onClick={() => void connect(s.url, s.name)}>
                  열기
                </button>
                <button type="button" className="icon-btn" aria-label={`${s.name} 지우기`} onClick={() => void forget(s.url)}>
                  <Trash2 size={15} />
                </button>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

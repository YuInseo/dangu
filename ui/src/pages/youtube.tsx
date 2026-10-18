import { useEffect, useRef, useState } from 'react';
import Link from '@/compat/link';
import { useSearchParams } from '@/compat/navigation';
import { ApiError, fetchJson, useTitle } from '@/api';
import { Bell, CircleAlert, FileAudio, LoaderCircle, Trash2, Upload } from '@/components/icons';
import { useToast } from '@/components/toast';
import {
  Avatar,
  ChannelItem,
  KeySetup,
  PlaylistCard,
  SubscriptionStrip,
  VideoCard,
  VideoGrid,
  WorldTabs,
  YtSearchBox,
} from '@/components/yt';
import {
  needsKey,
  refreshYt,
  setSubscribed,
  usePaged,
  useSubscriptions,
  useYtStatus,
  yt,
  type YtFeed,
  type YtSearchItem,
  type YtVideo,
} from '@/lib/youtube';
import { Loading } from './common';

function Problem({ error }: { error: unknown }) {
  if (needsKey(error)) return <KeySetup />;
  return (
    <div className="notice notice-error">
      <CircleAlert size={18} />
      <div>
        <strong>유튜브에서 불러오지 못했습니다.</strong>
        <div className="muted">{error instanceof Error ? error.message : String(error)}</div>
      </div>
    </div>
  );
}

function More({ show, loading, onClick }: { show: boolean; loading: boolean; onClick: () => void }) {
  if (!show) return null;
  return (
    <div className="yt-more">
      <button type="button" className="btn" disabled={loading} onClick={onClick}>
        {loading ? <LoaderCircle size={16} className="spin" /> : null} 더 보기
      </button>
    </div>
  );
}

/** 넓은 화면에서 끝에 닿으면 저절로 다음 쪽 */
function useAutoMore(enabled: boolean, more: () => void) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el || !enabled) return;
    const io = new IntersectionObserver((e) => e[0]?.isIntersecting && more(), { rootMargin: '600px' });
    io.observe(el);
    return () => io.disconnect();
  }, [enabled, more]);
  return ref;
}

// ---------------------------------------------------------------------------
// 홈: 구독 피드 · 인기 음악

export function YouTubePage() {
  useTitle('유튜브');
  // 다른 화면·기기에서 구독을 바꿨을 수 있으니 열 때마다 새로 묻는다(이 기기 안의 일이라 가볍다).
  useEffect(() => {
    void refreshYt();
  }, []);
  const status = useYtStatus();
  const subs = useSubscriptions();
  const [tab, setTab] = useState<'feed' | 'popular' | null>(null);
  const shown = tab ?? (subs && subs.length === 0 && status?.hasKey ? 'popular' : 'feed');

  return (
    <>
      <WorldTabs active="youtube">
        <YtSearchBox />
      </WorldTabs>
      <SubscriptionStrip />
      <div className="chips yt-chips">
        <button type="button" className={`chip${shown === 'feed' ? ' is-active' : ''}`} onClick={() => setTab('feed')}>
          구독
        </button>
        <button type="button" className={`chip${shown === 'popular' ? ' is-active' : ''}`} onClick={() => setTab('popular')}>
          인기 음악
        </button>
        <Link href="/youtube/subscriptions" className="chip">
          구독 관리
        </Link>
      </div>
      {shown === 'feed' ? <Feed /> : <Popular />}
      {status && !status.hasKey && shown === 'feed' && (
        <div className="section">
          <KeySetup />
        </div>
      )}
      <p className="credit">
        영상은 유튜브의 공식 플레이어로 재생됩니다(광고·자막·화질은 유튜브 그대로). 소리는 영상을 받거나 저장하지 않습니다.
      </p>
    </>
  );
}

function Feed() {
  const subs = useSubscriptions();
  const [pages, setPages] = useState<YtVideo[]>([]);
  const [next, setNext] = useState<number | undefined>(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [failed, setFailed] = useState<string[]>([]);
  const count = subs?.length ?? 0;

  const load = async (page: number) => {
    setLoading(true);
    try {
      const f = await fetchJson<YtFeed>(yt.feedUrl(page));
      setPages((p) => (page === 0 ? f.items : [...p, ...f.items]));
      setNext(f.next);
      setFailed(f.failed ?? []);
      setError(null);
    } catch (e) {
      setError(e);
    } finally {
      setLoading(false);
    }
  };
  useEffect(() => {
    if (count) void load(0);
    else setPages([]);
    // 구독을 더하거나 빼면 처음부터
  }, [count]);
  const sentinel = useAutoMore(next != null && !loading && pages.length > 0, () => next != null && void load(next));

  if (!subs) return <Loading />;
  if (!subs.length) {
    return (
      <div className="yt-empty">
        <Bell size={28} />
        <h2>구독한 채널이 없습니다</h2>
        <p className="muted">
          채널 화면에서 구독을 누르거나, 구글 테이크아웃의 구독 목록을 가져오세요. 구독한 채널의 새 영상이 여기에 모입니다(API 키 없이
          됩니다).
        </p>
        <Link href="/youtube/subscriptions" className="btn btn-sm">
          <Upload size={15} /> 구독 목록 가져오기
        </Link>
      </div>
    );
  }
  return (
    <>
      {error ? <Problem error={error} /> : null}
      {!pages.length && loading && <Loading />}
      <VideoGrid videos={pages} />
      {failed.length > 0 && <p className="muted yt-note">채널 {failed.length}곳의 새 영상을 읽지 못했습니다.</p>}
      <div ref={sentinel} />
      <More show={next != null} loading={loading} onClick={() => next != null && void load(next)} />
    </>
  );
}

function Popular() {
  const first = yt.popularUrl();
  const p = usePaged<YtVideo>(first, (t) => yt.popularUrl(t));
  const sentinel = useAutoMore(!!p.next && !p.loading, () => void p.more());
  if (p.error && !p.items.length) return <Problem error={p.error} />;
  if (!p.items.length) return <Loading />;
  return (
    <>
      <VideoGrid videos={p.items} />
      <div ref={sentinel} />
      <More show={!!p.next} loading={p.loading} onClick={() => void p.more()} />
    </>
  );
}

// ---------------------------------------------------------------------------
// 검색

const KINDS = [
  ['', '전체'],
  ['video', '동영상'],
  ['channel', '채널'],
  ['playlist', '재생목록'],
] as const;

export function YouTubeSearchPage() {
  const sp = useSearchParams();
  const q = (sp.get('q') ?? '').trim();
  const [kind, setKind] = useState('');
  useTitle(q ? `${q} — 유튜브` : '유튜브 검색');
  const p = usePaged<YtSearchItem>(q ? yt.searchUrl(q, undefined, kind || undefined) : null, (t) => yt.searchUrl(q, t, kind || undefined));
  const sentinel = useAutoMore(!!p.next && !p.loading, () => void p.more());
  return (
    <>
      <WorldTabs active="youtube">
        <YtSearchBox initial={q} />
      </WorldTabs>
      <div className="chips yt-chips">
        {KINDS.map(([k, label]) => (
          <button key={k} type="button" className={`chip${kind === k ? ' is-active' : ''}`} onClick={() => setKind(k)}>
            {label}
          </button>
        ))}
      </div>
      {p.error && !p.items.length ? <Problem error={p.error} /> : null}
      {!p.error && q && !p.items.length && p.loading && <Loading />}
      {!p.loading && !p.error && q && !p.items.length && <p className="muted pad">찾은 것이 없습니다.</p>}
      <SearchResults items={p.items} />
      <div ref={sentinel} />
      <More show={!!p.next} loading={p.loading} onClick={() => void p.more()} />
    </>
  );
}

export function SearchResults({ items }: { items: YtSearchItem[] }) {
  return (
    <div className="yt-results">
      {items.map((it) =>
        it.kind === 'channel' ? (
          <ChannelItem key={`c:${it.id}`} c={it} />
        ) : it.kind === 'playlist' ? (
          <PlaylistCard key={`p:${it.id}`} p={it} />
        ) : (
          <VideoCard key={`v:${it.id}`} v={it} />
        ),
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// 구독 관리 · 가져오기

export function YouTubeSubscriptionsPage() {
  useTitle('구독 관리');
  const subs = useSubscriptions();
  const toast = useToast();
  const file = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);

  const importFile = async (f: File) => {
    setBusy(true);
    try {
      const r = await yt.importCsv(await f.text());
      await refreshYt();
      toast(r.added ? `구독 ${r.added}개를 가져왔습니다(모두 ${r.count}개)` : '새로 더할 구독이 없습니다');
    } catch (e) {
      toast(e instanceof ApiError ? e.message : '가져오지 못했습니다', 'error');
    } finally {
      setBusy(false);
      if (file.current) file.current.value = '';
    }
  };

  return (
    <>
      <WorldTabs active="youtube" />
      <header className="page-head yt-subs-head">
        <h2 className="section-title">구독 {subs ? `${subs.length}개` : ''}</h2>
        <p className="page-lede">
          구독은 이 기기에 둡니다. 유튜브 계정의 구독을 옮기려면{' '}
          <a href="https://takeout.google.com/settings/takeout/custom/youtube" target="_blank" rel="noreferrer">
            구글 테이크아웃
          </a>
          에서 <b>YouTube 및 YouTube Music → 구독정보</b>만 골라 내보낸 뒤, 받은 파일 안의 <code>subscriptions.csv</code>를 가져오세요.
        </p>
        <input
          ref={file}
          type="file"
          accept=".csv,text/csv"
          hidden
          onChange={(e) => {
            const f = e.target.files?.[0];
            if (f) void importFile(f);
          }}
        />
        <button type="button" className="btn btn-sm" disabled={busy} onClick={() => file.current?.click()}>
          <FileAudio size={15} /> subscriptions.csv 가져오기
        </button>
      </header>
      {!subs ? (
        <Loading />
      ) : (
        <ul className="yt-sub-list">
          {subs.map((s) => (
            <li key={s.id}>
              <Link href={`/youtube/channel/${s.id}`} className="yt-sub-row">
                <Avatar src={s.thumb} name={s.title} size={44} />
                <span className="yt-sub-name">{s.title}</span>
              </Link>
              <button
                type="button"
                className="icon-btn"
                aria-label={`${s.title} 구독 끊기`}
                title="구독 끊기"
                onClick={() =>
                  setSubscribed(s, false)
                    .then(() => toast(`${s.title} 구독을 끊었습니다`))
                    .catch((e: unknown) => toast(e instanceof Error ? e.message : '끊지 못했습니다', 'error'))
                }
              >
                <Trash2 size={17} />
              </button>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

import { useEffect, useRef, useState } from 'react';
import Link from '@/compat/link';
import { useRouter, useSearchParams } from '@/compat/navigation';
import { ApiError, useTitle } from '@/api';
import { Bell, CircleAlert, FileAudio, Trash2, Upload } from '@/components/icons';
import { useToast } from '@/components/toast';
import {
  Avatar,
  ChannelItem,
  FreshPill,
  KeySetup,
  ListEnd,
  PlaylistCard,
  skeletonsFor,
  SubscriptionStrip,
  VideoCard,
  VideoGrid,
  VideoSkeletons,
  WorldTabs,
  YtSearchBox,
} from '@/components/yt';
import {
  needsKey,
  refreshYt,
  setSubscribed,
  subsKey,
  usePaged,
  useSubscriptions,
  useYtStatus,
  yt,
  type YtOrder,
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
  const router = useRouter();
  // 고른 칸은 주소에 둔다(영상을 보고 뒤로 오면 그 칸, 그 자리 그대로)
  const tab = useSearchParams().get('tab');
  const shown = tab === 'popular' || tab === 'feed' ? tab : subs && subs.length === 0 && status?.hasKey ? 'popular' : 'feed';
  const pick = (t: 'feed' | 'popular') => router.replace(`/youtube?tab=${t}`);

  return (
    <>
      <WorldTabs active="youtube">
        <YtSearchBox />
      </WorldTabs>
      <SubscriptionStrip />
      <div className="chips yt-chips">
        <button type="button" className={`chip${shown === 'feed' ? ' is-active' : ''}`} onClick={() => pick('feed')}>
          구독
        </button>
        <button type="button" className={`chip${shown === 'popular' ? ' is-active' : ''}`} onClick={() => pick('popular')}>
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

/** 구독한 채널의 새 영상, 새 것부터. 돌아오면 그 자리 그대로 보이고 뒤에서 새 영상을 확인한다. */
function Feed() {
  const subs = useSubscriptions();
  const v = subs ? subsKey(subs) : undefined;
  const p = usePaged<YtVideo>(subs?.length ? yt.feedUrl(undefined, v) : null, (t) => yt.feedUrl(t, v), { revalidate: 0 });
  const failed = (p.head?.failed as string[] | undefined) ?? [];

  if (!subs) return <VideoGrid videos={[]} skeletons={6} />;
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
      {p.fresh && <FreshPill onClick={p.showFresh} />}
      {p.error && !p.items.length ? <Problem error={p.error} /> : null}
      <VideoGrid videos={p.items} skeletons={skeletonsFor(p)} />
      {failed.length > 0 && <p className="muted yt-note">채널 {failed.length}곳의 새 영상을 읽지 못했습니다.</p>}
      <ListEnd p={p} />
    </>
  );
}

/** 지역의 인기 음악 차트, 새 것부터 */
function Popular() {
  const p = usePaged<YtVideo>(yt.popularUrl(), (t) => yt.popularUrl(t), { revalidate: 30 * 60_000 });
  if (p.error && !p.items.length) return <Problem error={p.error} />;
  return (
    <>
      {p.fresh && <FreshPill onClick={p.showFresh} />}
      <VideoGrid videos={p.items} skeletons={skeletonsFor(p)} />
      <ListEnd p={p} />
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

const ORDERS: [YtOrder, string][] = [
  ['date', '최신순'],
  ['relevance', '관련순'],
];

export function YouTubeSearchPage() {
  const sp = useSearchParams();
  const router = useRouter();
  const q = (sp.get('q') ?? '').trim();
  const kind = sp.get('type') ?? '';
  // 새 것부터가 기본
  const order: YtOrder = sp.get('order') === 'relevance' ? 'relevance' : 'date';
  const go = (k: string, o: YtOrder) => {
    const next = new URLSearchParams({ q });
    if (k) next.set('type', k);
    if (o !== 'date') next.set('order', o);
    router.replace(`/youtube/search?${next}`);
  };
  useTitle(q ? `${q} — 유튜브` : '유튜브 검색');
  const p = usePaged<YtSearchItem>(
    q ? yt.searchUrl(q, undefined, kind || undefined, order) : null,
    (t) => yt.searchUrl(q, t, kind || undefined, order),
    { revalidate: 30 * 60_000 },
  );
  return (
    <>
      <WorldTabs active="youtube">
        <YtSearchBox initial={q} />
      </WorldTabs>
      <div className="chips yt-chips">
        {KINDS.map(([k, label]) => (
          <button key={k} type="button" className={`chip${kind === k ? ' is-active' : ''}`} onClick={() => go(k, order)}>
            {label}
          </button>
        ))}
        <span className="yt-chips-gap" aria-hidden="true" />
        {ORDERS.map(([o, label]) => (
          <button key={o} type="button" className={`chip${order === o ? ' is-active' : ''}`} onClick={() => go(kind, o)}>
            {label}
          </button>
        ))}
      </div>
      {p.error && !p.items.length ? <Problem error={p.error} /> : null}
      {!p.loading && !p.error && q && !p.items.length && <p className="muted pad">찾은 것이 없습니다.</p>}
      <SearchResults items={p.items} skeletons={skeletonsFor(p)} />
      <ListEnd p={p} />
    </>
  );
}

export function SearchResults({ items, skeletons = 0 }: { items: YtSearchItem[]; skeletons?: number }) {
  return (
    <div className="yt-results" aria-busy={skeletons > 0 || undefined}>
      {items.map((it) =>
        it.kind === 'channel' ? (
          <ChannelItem key={`c:${it.id}`} c={it} />
        ) : it.kind === 'playlist' ? (
          <PlaylistCard key={`p:${it.id}`} p={it} />
        ) : (
          <VideoCard key={`v:${it.id}`} v={it} />
        ),
      )}
      <VideoSkeletons count={skeletons} />
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

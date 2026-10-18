'use client';

import { Fragment, useEffect, useState, type ReactNode } from 'react';
import Link from '@/compat/link';
import { useRouter } from '@/compat/navigation';
import { readJSON, writeJSON } from '@/lib/player/storage';
import { artColors } from '@/lib/hash';
import {
  agoIso,
  internalHref,
  isSubscribed,
  length,
  linkify,
  noteKey,
  setSubscribed,
  subsText,
  useSubscriptions,
  useYtStatus,
  viewsText,
  yt,
  type YtChannel,
  type YtPlaylist,
  type YtVideo,
} from '@/lib/youtube';
import { Bell, Check, ExternalLink, KeyRound, Link2, ListVideo, Search } from './icons';
import { useToast } from './toast';

// ---------------------------------------------------------------------------
// 월드: 유튜브 | 밴드

const WORLD_TAB = 'sori.world.tab';

export function worldHome(): string {
  return readJSON<string>(WORLD_TAB) === 'band' ? '/live' : '/youtube';
}

/** 월드 화면 맨 위: 이름과 유튜브 · 밴드 */
export function WorldTabs({ active, children }: { active: 'youtube' | 'band'; children?: ReactNode }) {
  useEffect(() => {
    writeJSON(WORLD_TAB, active);
  }, [active]);
  return (
    <div className="world-head">
      <div className="world-top">
        <h1 className="world-title">월드</h1>
        <nav className="seg world-tabs" aria-label="월드">
          <Link href="/youtube" className={`seg-item${active === 'youtube' ? ' is-active' : ''}`} replace scroll={false}>
            유튜브
          </Link>
          <Link href="/live" className={`seg-item${active === 'band' ? ' is-active' : ''}`} replace scroll={false}>
            밴드
          </Link>
        </nav>
      </div>
      {children}
    </div>
  );
}

// ---------------------------------------------------------------------------
// 조각

export function Avatar({ src, name, size = 36 }: { src?: string; name: string; size?: number }) {
  const [broken, setBroken] = useState(false);
  const { a, b } = artColors(name || '?');
  if (src && !broken) {
    return (
      <img
        className="yt-avatar"
        src={src}
        alt=""
        width={size}
        height={size}
        style={{ width: size, height: size }}
        loading="lazy"
        referrerPolicy="no-referrer"
        onError={() => setBroken(true)}
      />
    );
  }
  return (
    <span
      className="yt-avatar yt-avatar-letter"
      style={{
        width: size,
        height: size,
        fontSize: size * 0.42,
        background: `linear-gradient(135deg, hsl(${a} 55% 45%), hsl(${b} 55% 35%))`,
      }}
      aria-hidden
    >
      {[...(name.replace(/^@/, '') || '?')][0]?.toUpperCase()}
    </span>
  );
}

function thumbOf(v: YtVideo) {
  return v.thumb ?? `https://i.ytimg.com/vi/${v.id}/hqdefault.jpg`;
}

export function YtThumb({ v }: { v: YtVideo }) {
  const len = length(v.duration);
  return (
    <div className="yt-thumb">
      <img src={thumbOf(v)} alt="" loading="lazy" referrerPolicy="no-referrer" />
      {v.live === 'live' ? (
        <span className="yt-badge yt-badge-live">LIVE</span>
      ) : v.live === 'upcoming' ? (
        <span className="yt-badge">예정</span>
      ) : len ? (
        <span className="yt-badge">{len}</span>
      ) : null}
      {v.short && <span className="yt-badge yt-badge-short">Shorts</span>}
      {v.embeddable === false && (
        <span className="yt-badge yt-badge-ext" title="올린 사람이 유튜브 밖 재생을 꺼 둔 영상">
          <ExternalLink size={11} /> 유튜브에서만
        </span>
      )}
    </div>
  );
}

export function videoMeta(v: YtVideo, withChannel = true) {
  return [withChannel ? v.channelTitle : null, viewsText(v.views), v.live === 'live' ? '방송 중' : agoIso(v.published)]
    .filter(Boolean)
    .join(' · ');
}

/** 영상 하나(카드). 휴대폰에서는 화면 너비로, 넓은 화면에서는 그리드로 */
export function VideoCard({ v, channel = true }: { v: YtVideo; channel?: boolean }) {
  return (
    <Link href={`/youtube/watch/${v.id}`} className="yt-video" prefetch={false}>
      <YtThumb v={v} />
      <div className="yt-video-text">
        <div className="yt-video-title" title={v.title}>
          {v.title}
        </div>
        <div className="yt-video-meta">{videoMeta(v, channel)}</div>
      </div>
    </Link>
  );
}

export function VideoGrid({ videos, channel = true }: { videos: YtVideo[]; channel?: boolean }) {
  return (
    <div className="yt-grid">
      {videos.map((v) => (
        <VideoCard key={v.id} v={v} channel={channel} />
      ))}
    </div>
  );
}

export function SubscribeButton({ channel, small }: { channel: { id: string; title: string; thumb?: string }; small?: boolean }) {
  useSubscriptions();
  const on = isSubscribed(channel.id);
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  return (
    <button
      type="button"
      className={`btn ${small ? 'btn-sm ' : ''}${on ? 'yt-subbed' : 'yt-sub'}`}
      disabled={busy}
      aria-pressed={on}
      onClick={async (e) => {
        e.preventDefault();
        e.stopPropagation();
        setBusy(true);
        try {
          await setSubscribed(channel, !on);
          toast(on ? `${channel.title} 구독을 끊었습니다` : `${channel.title}을(를) 구독했습니다`);
        } catch (err) {
          toast(err instanceof Error ? err.message : '바꾸지 못했습니다', 'error');
        } finally {
          setBusy(false);
        }
      }}
    >
      {on ? (
        <>
          <Bell size={15} /> 구독중
        </>
      ) : (
        '구독'
      )}
    </button>
  );
}

export function ChannelItem({ c }: { c: YtChannel }) {
  return (
    <Link href={`/youtube/channel/${c.id}`} className="yt-channel-item" prefetch={false}>
      <Avatar src={c.thumb} name={c.title} size={84} />
      <div className="yt-channel-text">
        <div className="yt-channel-name">{c.title}</div>
        <div className="yt-video-meta">{[c.handle, subsText(c.subscribers)].filter(Boolean).join(' · ')}</div>
        {c.description && <div className="yt-channel-desc">{c.description}</div>}
      </div>
      <SubscribeButton channel={{ id: c.id, title: c.title, thumb: c.thumb }} small />
    </Link>
  );
}

export function PlaylistCard({ p }: { p: YtPlaylist }) {
  return (
    <Link href={`/youtube/playlist/${p.id}`} className="yt-video" prefetch={false}>
      <div className="yt-thumb yt-thumb-list">
        {p.thumb ? <img src={p.thumb} alt="" loading="lazy" referrerPolicy="no-referrer" /> : <div className="yt-thumb-empty" />}
        <span className="yt-badge">
          <ListVideo size={13} /> {p.count != null ? `${p.count}개` : '재생목록'}
        </span>
      </div>
      <div className="yt-video-text">
        <div className="yt-video-title">{p.title}</div>
        <div className="yt-video-meta">{['재생목록', p.channelTitle].filter(Boolean).join(' · ')}</div>
      </div>
    </Link>
  );
}

/** 설명·댓글: 주소는 누를 수 있게(유튜브 주소는 앱 안에서), 시각(1:23)은 그 자리로 */
export function RichText({ text, onTime }: { text: string; onTime?: (sec: number) => void }) {
  return (
    <>
      {linkify(text).map((p, i) => {
        if (p.t === 'text') return <Fragment key={i}>{p.v}</Fragment>;
        if (p.t === 'time')
          return onTime ? (
            <button key={i} type="button" className="yt-time" onClick={() => onTime(p.sec)}>
              {p.v}
            </button>
          ) : (
            <Fragment key={i}>{p.v}</Fragment>
          );
        const inside = internalHref(p.v);
        return inside ? (
          <Link key={i} href={inside} className="yt-link">
            {p.v}
          </Link>
        ) : (
          <a key={i} href={p.v} target="_blank" rel="noreferrer" className="yt-link">
            {p.v}
          </a>
        );
      })}
    </>
  );
}

// ---------------------------------------------------------------------------
// 검색 칸: 찾을 말이면 유튜브 검색, 링크면 그 영상·채널·재생목록

const looksLikeLink = (t: string) => /^(https?:\/\/|www\.|m\.|youtu\.?be|youtube\.com|@)/i.test(t.trim()) || /^[\w-]{11}$/.test(t.trim());

export function YtSearchBox({ initial = '', autoFocus }: { initial?: string; autoFocus?: boolean }) {
  const router = useRouter();
  const toast = useToast();
  const [q, setQ] = useState(initial);
  useEffect(() => setQ(initial), [initial]);
  const go = async () => {
    const t = q.trim();
    if (!t) return;
    if (looksLikeLink(t)) {
      try {
        const r = await yt.resolve(t);
        router.push(
          r.kind === 'video' ? `/youtube/watch/${r.id}` : r.kind === 'channel' ? `/youtube/channel/${r.id}` : `/youtube/playlist/${r.id}`,
        );
        return;
      } catch (e) {
        if (/^https?:\/\//i.test(t)) {
          toast(e instanceof Error ? e.message : '열 수 없는 링크입니다', 'error');
          return;
        }
      }
    }
    router.push(`/youtube/search?q=${encodeURIComponent(t)}`);
  };
  return (
    <form
      className="filter filter-lg yt-search"
      onSubmit={(e) => {
        e.preventDefault();
        void go();
      }}
    >
      {looksLikeLink(q) ? <Link2 size={18} /> : <Search size={18} />}
      <input
        value={q}
        onChange={(e) => setQ(e.target.value)}
        placeholder="유튜브 검색 또는 링크 붙여 넣기"
        aria-label="유튜브 검색"
        enterKeyHint="search"
        autoFocus={autoFocus}
        autoCapitalize="off"
        autoCorrect="off"
        spellCheck={false}
      />
      <button type="submit" className="btn btn-sm">
        찾기
      </button>
    </form>
  );
}

// ---------------------------------------------------------------------------
// API 키 넣기

export function KeySetup({ compact }: { compact?: boolean }) {
  const [key, setKey] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const toast = useToast();
  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      noteKey(await yt.setKey(key.trim()));
      setKey('');
      toast('유튜브 API 키를 넣었습니다');
    } catch (e) {
      setError(e instanceof Error ? e.message : '넣지 못했습니다');
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className={`yt-key${compact ? ' yt-key-compact' : ''}`}>
      <div className="yt-key-head">
        <KeyRound size={18} />
        <strong>검색·댓글·채널 정보는 유튜브 API 키가 있어야 합니다</strong>
      </div>
      {!compact && (
        <ol className="yt-key-steps">
          <li>
            <a href="https://console.cloud.google.com/apis/library/youtube.googleapis.com" target="_blank" rel="noreferrer">
              Google Cloud 콘솔 <ExternalLink size={12} />
            </a>
            에서 프로젝트를 만들고 <b>YouTube Data API v3</b>를 사용 설정합니다(무료).
          </li>
          <li>
            <a href="https://console.cloud.google.com/apis/credentials" target="_blank" rel="noreferrer">
              사용자 인증 정보 <ExternalLink size={12} />
            </a>
            → 사용자 인증 정보 만들기 → <b>API 키</b>. 만들어진 키(AIza…)를 아래에 붙여 넣습니다.
          </li>
          <li>하루에 검색 약 100번까지 무료입니다. 구독 피드와 영상 보기는 키가 없어도 됩니다.</li>
        </ol>
      )}
      <form
        className="yt-key-form"
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        <label className="filter">
          <KeyRound size={16} />
          <input
            value={key}
            onChange={(e) => setKey(e.target.value)}
            placeholder="AIza…"
            aria-label="유튜브 API 키"
            autoComplete="off"
            spellCheck={false}
          />
        </label>
        <button type="submit" className="btn btn-primary btn-sm" disabled={busy || key.trim().length < 20}>
          {busy ? '확인 중…' : '넣기'}
        </button>
      </form>
      {error && <p className="warn yt-key-error">{error}</p>}
    </div>
  );
}

/** 설정 → 연결: 넣은 키(끝 네 글자), 지우기, 또는 넣기 */
export function YouTubeKeyCard() {
  const status = useYtStatus();
  const toast = useToast();
  if (!status) return null;
  if (!status.hasKey) return <KeySetup />;
  return (
    <div className="pref-row">
      <div className="pref-label">API 키</div>
      <div className="pref-value yt-key-set">
        <span>
          <Check size={15} /> 넣어 두었습니다 <code>{status.keyHint}</code>
        </span>
        <button
          type="button"
          className="btn btn-sm"
          onClick={() =>
            yt
              .clearKey()
              .then((s) => {
                noteKey(s);
                toast('유튜브 API 키를 지웠습니다');
              })
              .catch(() => toast('지우지 못했습니다', 'error'))
          }
        >
          지우기
        </button>
      </div>
    </div>
  );
}

/** 구독한 채널 줄(유튜브 앱 맨 위의 동그라미들) */
export function SubscriptionStrip() {
  const subs = useSubscriptions();
  if (!subs?.length) return null;
  return (
    <div className="yt-strip" role="list">
      {subs.map((s) => (
        <Link key={s.id} href={`/youtube/channel/${s.id}`} className="yt-strip-item" role="listitem" prefetch={false}>
          <Avatar src={s.thumb} name={s.title} size={56} />
          <span>{s.title}</span>
        </Link>
      ))}
      <Link href="/youtube/subscriptions" className="yt-strip-item yt-strip-all">
        <span className="yt-avatar yt-avatar-letter yt-strip-more" style={{ width: 56, height: 56 }}>
          <Check size={20} />
        </span>
        <span>관리</span>
      </Link>
    </div>
  );
}

import { useRef, useState } from 'react';
import { useParams } from 'react-router';
import Link from '@/compat/link';
import { useSearchParams } from '@/compat/navigation';
import { useApi, useTitle } from '@/api';
import { ChevronDown, ChevronUp, ExternalLink, LoaderCircle, MessageSquareText, ThumbsUp } from '@/components/icons';
import { Avatar, KeySetup, RichText, SubscribeButton, videoMeta, WorldTabs, YtThumb } from '@/components/yt';
import { YtPlayer, type YtPlayerHandle } from '@/components/yt-player';
import {
  agoIso,
  compact,
  needsKey,
  subsText,
  usePaged,
  viewsText,
  yt,
  type YtChannelDetail,
  type YtComment,
  type YtVideo,
  type YtVideoDetail,
} from '@/lib/youtube';
import { Loading } from './common';

export function YouTubeWatchPage() {
  const { id = '' } = useParams();
  const sp = useSearchParams();
  const list = sp.get('list') ?? undefined;
  const t = Number(sp.get('t')) || undefined;
  const { data, error } = useApi<YtVideoDetail>(yt.videoUrl(id));
  useTitle(data?.video.title ?? '유튜브');
  const player = useRef<YtPlayerHandle>(null);
  const v = data?.video;
  const ch = data?.channel;
  const seek = (sec: number) => player.current?.seek(sec);

  return (
    <div className="yt-watch">
      <div className="yt-watch-main">
        <YtPlayer
          key={id}
          id={id}
          list={list}
          start={t}
          blocked={v?.embeddable === false}
          thumb={v?.thumb}
          title={v?.title}
          channel={ch?.title ?? v?.channelTitle}
          ref={player}
        />
        {error && !data && (
          <div className="notice notice-error yt-watch-error">
            <div>
              <strong>{error.status === 404 ? '영상을 찾을 수 없습니다' : '영상 정보를 불러오지 못했습니다'}</strong>
              <div className="muted">{error.message}</div>
            </div>
          </div>
        )}
        {!data && !error && <Loading />}
        {v && (
          <>
            {v.title && <h1 className="yt-watch-title">{v.title}</h1>}
            <div className="yt-watch-bar">
              {ch ? (
                <Link href={`/youtube/channel/${ch.id}`} className="yt-watch-channel">
                  <Avatar src={ch.thumb} name={ch.title} size={40} />
                  <span>
                    <strong>{ch.title}</strong>
                    {ch.subscribers != null && <span className="yt-video-meta">{subsText(ch.subscribers)}</span>}
                  </span>
                </Link>
              ) : v.channelTitle ? (
                <span className="yt-watch-channel">
                  <Avatar name={v.channelTitle} size={40} />
                  <strong>{v.channelTitle}</strong>
                </span>
              ) : null}
              {ch && <SubscribeButton channel={{ id: ch.id, title: ch.title, thumb: ch.thumb }} />}
              <div className="yt-watch-pills">
                {v.likes != null && (
                  <span className="yt-pill">
                    <ThumbsUp size={15} /> {compact(v.likes)}
                  </span>
                )}
                <a className="yt-pill" href={`https://www.youtube.com/watch?v=${v.id}`} target="_blank" rel="noreferrer">
                  <ExternalLink size={15} /> 유튜브에서 열기
                </a>
              </div>
            </div>
            <Description v={v} onTime={seek} limited={data.limited} />
            {data.limited ? (
              <div className="section">
                <p className="muted yt-note">조회수·설명·댓글은 유튜브 API 키를 넣으면 보입니다.</p>
                <KeySetup compact />
              </div>
            ) : (
              <Comments videoId={v.id} count={v.comments} onTime={seek} />
            )}
          </>
        )}
      </div>
      <aside className="yt-watch-side">{ch && <MoreFrom channelId={ch.id} current={id} />}</aside>
    </div>
  );
}

function Description({ v, onTime, limited }: { v: YtVideo; onTime: (s: number) => void; limited: boolean }) {
  const [open, setOpen] = useState(false);
  const meta = [viewsText(v.views), v.live === 'live' ? '방송 중' : agoIso(v.published)].filter(Boolean).join(' · ');
  if (limited && !v.description) return null;
  return (
    <div className={`yt-desc${open ? ' is-open' : ''}`}>
      {meta && <div className="yt-desc-meta">{meta}</div>}
      {v.description && (
        <div className="yt-desc-text">
          <RichText text={v.description} onTime={onTime} />
        </div>
      )}
      {v.description && v.description.length > 160 && (
        <button type="button" className="yt-desc-toggle" onClick={() => setOpen(!open)}>
          {open ? '간략히' : '더보기'}
        </button>
      )}
    </div>
  );
}

function Comments({ videoId, count, onTime }: { videoId: string; count?: number; onTime: (s: number) => void }) {
  const [order, setOrder] = useState<'relevance' | 'time'>('relevance');
  const p = usePaged<YtComment>(yt.commentsUrl(videoId, order), (t) => yt.commentsUrl(videoId, order, t), { revalidate: 5 * 60_000 });
  return (
    <section className="section yt-comments">
      <div className="section-head">
        <h2 className="section-title">
          <MessageSquareText size={19} /> 댓글 {count != null ? `${compact(count)}개` : ''}
        </h2>
        <div className="seg yt-comment-sort">
          <button type="button" className={`seg-item${order === 'relevance' ? ' is-active' : ''}`} onClick={() => setOrder('relevance')}>
            인기순
          </button>
          <button type="button" className={`seg-item${order === 'time' ? ' is-active' : ''}`} onClick={() => setOrder('time')}>
            최신순
          </button>
        </div>
      </div>
      {p.error && !p.items.length ? (
        needsKey(p.error) ? (
          <KeySetup compact />
        ) : (
          <p className="muted">{p.error instanceof Error ? p.error.message : '댓글을 불러오지 못했습니다'}</p>
        )
      ) : null}
      {!p.items.length && p.loading && <Loading />}
      <ul className="yt-comment-list">
        {p.items.map((c) => (
          <CommentItem key={c.id} c={c} onTime={onTime} />
        ))}
      </ul>
      {p.next && (
        <div className="yt-more">
          <button type="button" className="btn" disabled={p.loading} onClick={() => void p.more()}>
            {p.loading ? <LoaderCircle size={16} className="spin" /> : null} 댓글 더 보기
          </button>
        </div>
      )}
    </section>
  );
}

function CommentItem({ c, onTime, reply }: { c: YtComment; onTime: (s: number) => void; reply?: boolean }) {
  const [open, setOpen] = useState(false);
  return (
    <li className={`yt-comment${reply ? ' yt-reply' : ''}`}>
      {c.authorChannelId ? (
        <Link href={`/youtube/channel/${c.authorChannelId}`} className="yt-comment-avatar">
          <Avatar src={c.authorAvatar} name={c.author} size={reply ? 28 : 36} />
        </Link>
      ) : (
        <span className="yt-comment-avatar">
          <Avatar src={c.authorAvatar} name={c.author} size={reply ? 28 : 36} />
        </span>
      )}
      <div className="yt-comment-body">
        <div className="yt-comment-head">
          <strong>{c.author}</strong>
          <span className="yt-video-meta">
            {agoIso(c.published)}
            {c.edited ? ' (수정됨)' : ''}
          </span>
        </div>
        <div className="yt-comment-text">
          <RichText text={c.text} onTime={onTime} />
        </div>
        <div className="yt-comment-foot">
          <span>
            <ThumbsUp size={13} /> {c.likes ? compact(c.likes) : ''}
          </span>
        </div>
        {!reply && c.replies > 0 && (
          <>
            <button type="button" className="yt-replies-toggle" onClick={() => setOpen(!open)}>
              {open ? <ChevronUp size={16} /> : <ChevronDown size={16} />} 답글 {compact(c.replies)}개
            </button>
            {open && <Replies parent={c} onTime={onTime} />}
          </>
        )}
      </div>
    </li>
  );
}

function Replies({ parent, onTime }: { parent: YtComment; onTime: (s: number) => void }) {
  const p = usePaged<YtComment>(yt.repliesUrl(parent.id), (t) => yt.repliesUrl(parent.id, t));
  const items = p.items.length ? p.items : (parent.replyPreview ?? []);
  return (
    <ul className="yt-comment-list yt-replies">
      {items.map((r) => (
        <CommentItem key={r.id} c={r} onTime={onTime} reply />
      ))}
      {p.loading && <LoaderCircle size={16} className="spin" />}
      {p.next && (
        <button type="button" className="yt-replies-toggle" onClick={() => void p.more()}>
          답글 더 보기
        </button>
      )}
    </ul>
  );
}

/** 오른쪽(휴대폰은 아래): 이 채널의 다른 영상. 키가 없어도 채널 RSS로 된다. */
function MoreFrom({ channelId, current }: { channelId: string; current: string }) {
  const { data } = useApi<YtChannelDetail>(yt.channelUrl(channelId));
  const items = (data?.videos.items ?? []).filter((v) => v.id !== current).slice(0, 12);
  if (!items.length) return null;
  return (
    <div className="yt-side-list">
      <h3 className="yt-side-title">{data?.channel.title}의 다른 영상</h3>
      {items.map((v) => (
        <Link key={v.id} href={`/youtube/watch/${v.id}`} className="yt-side-item" prefetch={false}>
          <YtThumb v={v} />
          <div className="yt-video-text">
            <div className="yt-video-title">{v.title}</div>
            <div className="yt-video-meta">{videoMeta(v, false)}</div>
          </div>
        </Link>
      ))}
    </div>
  );
}

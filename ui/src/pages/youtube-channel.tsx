import { useEffect, useState } from 'react';
import { useParams } from 'react-router';
import Link from '@/compat/link';
import { fetchJson, useApi, useTitle } from '@/api';
import { ExternalLink, LoaderCircle, Play } from '@/components/icons';
import { Avatar, KeySetup, PlaylistCard, SubscribeButton, VideoCard, VideoGrid, WorldTabs } from '@/components/yt';
import {
  compact,
  needsKey,
  subsText,
  usePaged,
  yt,
  type YtChannelDetail,
  type YtPlaylist,
  type YtPlaylistDetail,
  type YtVideo,
} from '@/lib/youtube';
import { Failed, Loading } from './common';

export function YouTubeChannelPage() {
  const { id = '' } = useParams();
  const { data, error } = useApi<YtChannelDetail>(yt.channelUrl(id));
  const [tab, setTab] = useState<'videos' | 'playlists'>('videos');
  useTitle(data?.channel.title ?? '채널');
  useEffect(() => setTab('videos'), [id]);
  if (error && !data) return <Failed error={error} />;
  if (!data) return <Loading />;
  const c = data.channel;

  return (
    <>
      <WorldTabs active="youtube" />
      {c.banner && (
        <div className="yt-banner">
          <img src={c.banner} alt="" referrerPolicy="no-referrer" />
        </div>
      )}
      <header className="yt-channel-head">
        <Avatar src={c.thumb} name={c.title} size={88} />
        <div className="yt-channel-info">
          <h1 className="yt-channel-title">{c.title}</h1>
          <div className="yt-video-meta">
            {[c.handle, subsText(c.subscribers), c.videos != null ? `동영상 ${compact(c.videos)}개` : null].filter(Boolean).join(' · ')}
          </div>
          {c.description && <p className="yt-channel-about">{c.description}</p>}
          <div className="yt-channel-actions">
            <SubscribeButton channel={{ id: c.id, title: c.title, thumb: c.thumb }} />
            <a className="btn btn-sm" href={`https://www.youtube.com/channel/${c.id}`} target="_blank" rel="noreferrer">
              <ExternalLink size={15} /> 유튜브에서 열기
            </a>
          </div>
        </div>
      </header>

      <div className="seg yt-channel-tabs">
        <button type="button" className={`seg-item${tab === 'videos' ? ' is-active' : ''}`} onClick={() => setTab('videos')}>
          동영상
        </button>
        {!data.limited && (
          <button type="button" className={`seg-item${tab === 'playlists' ? ' is-active' : ''}`} onClick={() => setTab('playlists')}>
            재생목록
          </button>
        )}
      </div>

      {tab === 'videos' ? <ChannelVideos detail={data} /> : <ChannelPlaylists id={c.id} />}
      {data.limited && (
        <div className="section">
          <p className="muted yt-note">API 키 없이 채널 피드로 최근 영상 {data.videos.items.length}개만 보입니다.</p>
          <KeySetup compact />
        </div>
      )}
    </>
  );
}

function ChannelVideos({ detail }: { detail: YtChannelDetail }) {
  const [more, setMore] = useState<YtVideo[]>([]);
  const [next, setNext] = useState(detail.videos.next);
  const [loading, setLoading] = useState(false);
  useEffect(() => {
    setMore([]);
    setNext(detail.videos.next);
  }, [detail]);
  const load = async () => {
    if (!next) return;
    setLoading(true);
    try {
      const r = await fetchJson<{ items: YtVideo[]; next?: string }>(yt.channelVideosUrl(detail.channel.id, next));
      setMore((m) => [...m, ...r.items]);
      setNext(r.next);
    } finally {
      setLoading(false);
    }
  };
  const videos = [...detail.videos.items, ...more];
  if (!videos.length) return <p className="muted pad">올린 영상이 없습니다.</p>;
  return (
    <>
      <VideoGrid videos={videos} channel={false} />
      {next && (
        <div className="yt-more">
          <button type="button" className="btn" disabled={loading} onClick={() => void load()}>
            {loading ? <LoaderCircle size={16} className="spin" /> : null} 더 보기
          </button>
        </div>
      )}
    </>
  );
}

function ChannelPlaylists({ id }: { id: string }) {
  const p = usePaged<YtPlaylist>(yt.channelPlaylistsUrl(id), (t) => yt.channelPlaylistsUrl(id, t));
  if (p.error && !p.items.length) return needsKey(p.error) ? <KeySetup /> : <p className="muted pad">재생목록을 불러오지 못했습니다.</p>;
  if (!p.items.length) return p.loading ? <Loading /> : <p className="muted pad">재생목록이 없습니다.</p>;
  return (
    <>
      <div className="yt-grid">
        {p.items.map((pl) => (
          <PlaylistCard key={pl.id} p={pl} />
        ))}
      </div>
      {p.next && (
        <div className="yt-more">
          <button type="button" className="btn" disabled={p.loading} onClick={() => void p.more()}>
            더 보기
          </button>
        </div>
      )}
    </>
  );
}

export function YouTubePlaylistPage() {
  const { id = '' } = useParams();
  const { data, error } = useApi<YtPlaylistDetail>(yt.playlistUrl(id));
  useTitle(data?.playlist.title ?? '재생목록');
  const more = usePaged<YtVideo>(data?.videos.next ? yt.playlistUrl(id, data.videos.next) : null, (t) => yt.playlistUrl(id, t));
  if (error && !data)
    return needsKey(error) ? (
      <>
        <WorldTabs active="youtube" />
        <KeySetup />
      </>
    ) : (
      <Failed error={error} />
    );
  if (!data) return <Loading />;
  const pl = data.playlist;
  const videos = [...data.videos.items, ...more.items];
  return (
    <>
      <WorldTabs active="youtube" />
      <header className="yt-playlist-head">
        <div className="yt-playlist-title">
          <div className="eyebrow">재생목록</div>
          <h1 className="page-title">{pl.title}</h1>
          <div className="page-meta">{[pl.channelTitle, pl.count != null ? `동영상 ${pl.count}개` : null].filter(Boolean).join(' · ')}</div>
        </div>
        {videos[0] && (
          <Link href={`/youtube/watch/${videos[0].id}?list=${encodeURIComponent(pl.id)}`} className="btn btn-primary">
            <Play size={18} fill="currentColor" /> 모두 재생
          </Link>
        )}
      </header>
      <div className="yt-results">
        {videos.map((v) => (
          <VideoCard key={v.id} v={v} />
        ))}
      </div>
    </>
  );
}

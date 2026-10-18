import { useEffect, useState } from 'react';
import { useParams } from 'react-router';
import Link from '@/compat/link';
import { useApi, useTitle } from '@/api';
import { ExternalLink, Play } from '@/components/icons';
import {
  Avatar,
  KeySetup,
  ListEnd,
  PlaylistCard,
  skeletonsFor,
  SubscribeButton,
  VideoCard,
  VideoGrid,
  VideoSkeletons,
  WorldTabs,
} from '@/components/yt';
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
  const id = detail.channel.id;
  // 키가 없으면 채널 피드의 최근 영상뿐(다음 쪽 없음)
  const p = usePaged<YtVideo>(detail.limited ? null : yt.channelVideosUrl(id), (t) => yt.channelVideosUrl(id, t), {
    seed: detail.videos,
    revalidate: 15 * 60_000,
  });
  const videos = detail.limited ? detail.videos.items : p.items;
  if (!videos.length && !p.loading) return <p className="muted pad">올린 영상이 없습니다.</p>;
  return (
    <>
      <VideoGrid videos={videos} channel={false} skeletons={skeletonsFor(p)} />
      <ListEnd p={p} />
    </>
  );
}

function ChannelPlaylists({ id }: { id: string }) {
  const p = usePaged<YtPlaylist>(yt.channelPlaylistsUrl(id), (t) => yt.channelPlaylistsUrl(id, t));
  if (p.error && !p.items.length) return needsKey(p.error) ? <KeySetup /> : <p className="muted pad">재생목록을 불러오지 못했습니다.</p>;
  if (!p.items.length && !p.loading) return <p className="muted pad">재생목록이 없습니다.</p>;
  return (
    <>
      <div className="yt-grid">
        {p.items.map((pl) => (
          <PlaylistCard key={pl.id} p={pl} />
        ))}
        <VideoSkeletons count={skeletonsFor(p)} />
      </div>
      <ListEnd p={p} />
    </>
  );
}

export function YouTubePlaylistPage() {
  const { id = '' } = useParams();
  const { data, error } = useApi<YtPlaylistDetail>(yt.playlistUrl(id));
  useTitle(data?.playlist.title ?? '재생목록');
  // 쪽마다 { playlist, videos }가 온다
  const p = usePaged<YtVideo>(data ? yt.playlistUrl(id) : null, (t) => yt.playlistUrl(id, t), {
    seed: data?.videos,
    pick: (raw) => (raw as YtPlaylistDetail).videos,
  });
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
  const videos = p.items.length ? p.items : data.videos.items;
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
        <VideoSkeletons count={skeletonsFor(p)} />
      </div>
      <ListEnd p={p} />
    </>
  );
}

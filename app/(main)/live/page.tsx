import Link from 'next/link';
import type { Metadata } from 'next';
import { LiveShowCard } from '@/components/live-show-card';
import { LiveSearch } from '@/components/live-search';
import { CircleAlert, Radio } from '@/components/icons';
import { searchShows } from '@/lib/live/archive';
import { LIVE_SORT_LABEL, type LiveSort } from '@/lib/live/shared';
import { count } from '@/lib/time';

export const metadata: Metadata = { title: '라이브 아카이브' };

const ROWS = 48;

export default async function LivePage({ searchParams }: { searchParams: Promise<{ q?: string; sort?: string; page?: string }> }) {
  const sp = await searchParams;
  const q = (sp.q ?? '').trim();
  const sort: LiveSort = sp.sort === 'recent' || sp.sort === 'newest' ? sp.sort : 'popular';
  const page = Math.max(1, Number(sp.page) || 1);

  let result: Awaited<ReturnType<typeof searchShows>> | null = null;
  let error: string | null = null;
  try {
    result = await searchShows({ q, sort, page, rows: ROWS });
  } catch (e) {
    error = e instanceof Error ? e.message : String(e);
  }

  const href = (p: { sort?: LiveSort; page?: number }) => {
    const s = new URLSearchParams();
    if (q) s.set('q', q);
    const so = p.sort ?? sort;
    if (so !== 'popular') s.set('sort', so);
    if (p.page && p.page > 1) s.set('page', String(p.page));
    const str = s.toString();
    return str ? `/live?${str}` : '/live';
  };
  const pages = result ? Math.ceil(result.total / ROWS) : 0;

  return (
    <>
      <header className="page-head live-head">
        <div className="eyebrow">
          <Radio size={13} /> 무료 스트리밍
        </div>
        <h1 className="page-title">라이브 아카이브</h1>
        <p className="page-lede">
          Internet Archive의 Live Music Archive. 아티스트가 녹음과 무료 공유를 허락한 공연 실황이고, 대부분 원본이 무손실 FLAC입니다. 가입도
          광고도 없이 원본 파일을 그대로 받아 재생합니다.
        </p>
        <LiveSearch initial={q} />
      </header>

      <div className="toolbar">
        <div className="seg" role="tablist" aria-label="정렬">
          {(Object.keys(LIVE_SORT_LABEL) as LiveSort[]).map((k) => (
            <Link key={k} href={href({ sort: k })} className={`seg-item${k === sort ? ' is-active' : ''}`} replace scroll={false}>
              {LIVE_SORT_LABEL[k]}
            </Link>
          ))}
        </div>
        {result && (
          <span className="muted">
            {q ? `“${q}” ` : ''}공연 {count(result.total)}개
          </span>
        )}
      </div>

      {error && (
        <div className="notice notice-error">
          <CircleAlert size={18} />
          <div>
            <strong>archive.org에 닿지 못했습니다.</strong>
            <div className="muted">서버가 인터넷에 나갈 수 있는지 확인하세요. ({error})</div>
          </div>
        </div>
      )}

      {result && result.shows.length === 0 && <p className="muted pad">찾은 공연이 없습니다. 밴드 이름을 영어로 쳐 보세요.</p>}

      {result && result.shows.length > 0 && (
        <div className="grid">
          {result.shows.map((s) => (
            <LiveShowCard key={s.id} show={s} />
          ))}
        </div>
      )}

      {pages > 1 && (
        <nav className="pager" aria-label="쪽">
          {page > 1 ? (
            <Link href={href({ page: page - 1 })} className="btn">
              이전
            </Link>
          ) : (
            <span />
          )}
          <span className="muted">
            {page} / {count(pages)}
          </span>
          {page < pages ? (
            <Link href={href({ page: page + 1 })} className="btn">
              다음
            </Link>
          ) : (
            <span />
          )}
        </nav>
      )}
      <p className="credit">
        음원과 정보는{' '}
        <a href="https://archive.org/details/etree" target="_blank" rel="noreferrer">
          archive.org/details/etree
        </a>
        에서 옵니다. 각 공연의 공유 조건은 해당 페이지를 따릅니다.
      </p>
    </>
  );
}

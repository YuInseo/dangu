'use client';

import { useState } from 'react';
import Link from '@/compat/link';
import { useRouter } from '@/compat/navigation';
import { isActive, saveFraction, saveText, savesApi, useSaveJob, useSaveJobs, type SaveJob } from '@/lib/live/saves';
import type { LiveShowDetail } from '@/lib/live/shared';
import { bytes } from '@/lib/time';
import { CircleCheck, Download, Link2, Lock, X } from './icons';
import { Section } from './section';
import { useToast } from './toast';

function Bar({ job }: { job: SaveJob }) {
  return (
    <div className="bar save-bar">
      <span style={{ width: `${Math.round(saveFraction(job) * 100)}%` }} />
    </div>
  );
}

/** 공연 화면: 라이브러리에 저장. 아티스트가 듣기만 허락한 공연은 까닭만 적는다. */
export function LiveSave({ show }: { show: LiveShowDetail }) {
  const { job, setJob } = useSaveJob(show.id);
  const [busy, setBusy] = useState(false);
  const toast = useToast();

  if (!show.saveable) {
    return (
      <p className="save-note">
        <Lock size={14} /> {show.saveBlock ?? '이 공연은 저장할 수 없습니다'}
      </p>
    );
  }

  const total = show.tracks.length && show.tracks.every((t) => t.size) ? show.tracks.reduce((s, t) => s + (t.size ?? 0), 0) : 0;
  const start = async () => {
    setBusy(true);
    try {
      setJob(await savesApi.save(show.id));
    } catch (e) {
      toast(e instanceof Error ? e.message : '저장하지 못했습니다', 'error');
    } finally {
      setBusy(false);
    }
  };
  const cancel = () =>
    savesApi
      .cancel(show.id)
      .then(setJob)
      .catch(() => {});

  if (job && isActive(job)) {
    return (
      <div className="save-box" role="status">
        <div className="save-line">
          <Download size={16} className="save-icon" />
          <span className="save-text">{saveText(job)}</span>
          <button type="button" className="btn btn-sm" onClick={() => void cancel()}>
            취소
          </button>
        </div>
        <Bar job={job} />
      </div>
    );
  }

  if (job?.state === 'done') {
    return (
      <div className="save-box" role="status">
        <div className="save-line">
          <CircleCheck size={16} className="save-icon save-ok" />
          <span className="save-text">라이브러리에 저장했습니다</span>
          {job.albumId ? (
            <Link href={`/albums/${job.albumId}`} className="btn btn-sm">
              보관함에서 보기
            </Link>
          ) : (
            <span className="muted">라이브러리를 읽는 중…</span>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="save-box">
      {job?.state === 'failed' && <p className="warn save-error">{saveText(job)}</p>}
      <div className="save-line">
        <button type="button" className="btn btn-sm" disabled={busy} onClick={() => void start()}>
          <Download size={15} /> {job?.state === 'failed' ? '이어서 저장' : '라이브러리에 저장'}
          {total ? <span className="save-size">{bytes(total)}</span> : null}
        </button>
      </div>
      <p className="save-hint">아티스트가 공유를 허락한 공연이라 원본 그대로 음악 폴더에 저장합니다.</p>
    </div>
  );
}

/** 라이브 화면 위: archive.org 공연 링크로 열기 */
export function LiveLink() {
  const router = useRouter();
  const [url, setUrl] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const go = async () => {
    if (!url.trim()) return;
    setBusy(true);
    setError(null);
    try {
      const r = await savesApi.resolve(url.trim());
      router.push(`/live/${encodeURIComponent(r.id)}`);
    } catch (e) {
      setError(e instanceof Error ? e.message : '열 수 없습니다');
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="live-link">
      <form
        className="filter"
        onSubmit={(e) => {
          e.preventDefault();
          void go();
        }}
      >
        <Link2 size={17} />
        <input
          value={url}
          onChange={(e) => {
            setUrl(e.target.value);
            setError(null);
          }}
          placeholder="archive.org 공연 링크로 열기"
          aria-label="archive.org 링크"
          inputMode="url"
          autoCapitalize="off"
          autoCorrect="off"
          spellCheck={false}
        />
        <button type="submit" className="btn btn-sm" disabled={busy || !url.trim()}>
          열기
        </button>
      </form>
      {error && <p className="warn live-link-error">{error}</p>}
    </div>
  );
}

/** 라이브 화면: 저장하는 중이거나 저장한 공연 */
export function SaveList() {
  const { jobs, setJobs } = useSaveJobs();
  if (!jobs.length) return null;
  const update = (j: SaveJob) => setJobs((all) => all.map((x) => (x.id === j.id ? j : x)));
  const dismiss = (id: string) => {
    setJobs((all) => all.filter((x) => x.id !== id));
    void savesApi.dismiss(id);
  };
  return (
    <Section title="저장한 공연" sub="음악 폴더 / Live Archive">
      <ul className="save-list">
        {[...jobs].reverse().map((j) => (
          <li key={j.id} className={`save-item save-${j.state}`}>
            <Link href={`/live/${encodeURIComponent(j.id)}`} className="save-item-main">
              <span className="save-item-title">
                <strong>{j.band}</strong> {j.album}
              </span>
              <span className={j.state === 'failed' ? 'warn' : 'muted'}>{saveText(j)}</span>
              {isActive(j) && <Bar job={j} />}
            </Link>
            {isActive(j) ? (
              <button
                type="button"
                className="btn btn-sm"
                onClick={() =>
                  savesApi
                    .cancel(j.id)
                    .then(update)
                    .catch(() => {})
                }
              >
                취소
              </button>
            ) : (
              <>
                {j.state === 'done' && j.albumId && (
                  <Link href={`/albums/${j.albumId}`} className="btn btn-sm">
                    보관함
                  </Link>
                )}
                <button
                  type="button"
                  className="icon-btn"
                  aria-label="목록에서 지우기"
                  title="목록에서 지우기(파일은 그대로)"
                  onClick={() => dismiss(j.id)}
                >
                  <X size={16} />
                </button>
              </>
            )}
          </li>
        ))}
      </ul>
    </Section>
  );
}

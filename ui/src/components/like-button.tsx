'use client';

import { setLiked, useLiked, type LikeKind } from '@/lib/collections';
import { Heart } from './icons';
import { useToast } from './toast';

/** ♥. 누르면 곧바로 바뀌고, 서버가 받지 못하면 되돌린다. */
export function LikeButton({
  kind,
  id,
  size = 20,
  className = '',
  name,
}: {
  kind: LikeKind;
  id: string | undefined;
  size?: number;
  className?: string;
  /** 화면 읽기 프로그램이 읽을 대상 이름 */
  name?: string;
}) {
  const on = useLiked(kind, id);
  const toast = useToast();
  if (!id) return null;
  const label = on ? '좋아요 취소' : '좋아요';
  return (
    <button
      type="button"
      className={`icon-btn like-btn${on ? ' is-liked' : ''}${className ? ` ${className}` : ''}`}
      aria-pressed={on}
      aria-label={label}
      title={name ? `${name} ${label}` : label}
      onClick={(e) => {
        e.stopPropagation();
        e.preventDefault();
        setLiked(kind, id, !on).catch(() => toast('좋아요를 저장하지 못했습니다', 'error'));
      }}
    >
      <Heart size={size} fill={on ? 'currentColor' : 'none'} />
    </button>
  );
}

'use client';

import { useEffect, useRef, useState, type CSSProperties } from 'react';
import { artColors } from '@/lib/hash';

/**
 * 표지가 없을 때 그리는 그림. 공연 포스터처럼 이름과 날짜를 큼직하게 얹는다.
 */
export function GeneratedArt({
  seed,
  title,
  subtitle,
  size = 'md',
}: {
  seed: string;
  title: string;
  subtitle?: string;
  size?: 'sm' | 'md' | 'lg';
}) {
  const { a, b, angle } = artColors(seed);
  const style = {
    background: `radial-gradient(120% 90% at 85% 10%, hsl(${b} 62% 46% / .85), transparent 60%), linear-gradient(${angle}deg, hsl(${a} 48% 22%), hsl(${b} 42% 12%))`,
  } as CSSProperties;
  return (
    <div className={`art art-${size}`} style={style} aria-hidden>
      {size !== 'sm' && (
        <div className="art-text">
          <span className="art-title">{title}</span>
          {subtitle && <span className="art-sub">{subtitle}</span>}
        </div>
      )}
    </div>
  );
}

interface CoverProps {
  src?: string;
  color?: string;
  alt: string;
  seed: string;
  title: string;
  subtitle?: string;
  size?: 'sm' | 'md' | 'lg';
  round?: boolean;
  className?: string;
  priority?: boolean;
}

/** 앨범 표지. 뜨기 전에는 표지의 대표 색으로 자리를 채운다. */
export function Cover({ src, color, alt, seed, title, subtitle, size = 'md', round, className, priority }: CoverProps) {
  const [loaded, setLoaded] = useState(false);
  const [failed, setFailed] = useState(false);
  const ref = useRef<HTMLImageElement>(null);

  useEffect(() => {
    setLoaded(false);
    setFailed(false);
    // 하이드레이션 전에 이미 다 받은 그림은 onLoad가 오지 않는다.
    if (ref.current?.complete && ref.current.naturalWidth) setLoaded(true);
  }, [src]);

  return (
    <div
      className={`cover${round ? ' cover-round' : ''}${className ? ` ${className}` : ''}`}
      style={{ '--cover-color': color ?? 'transparent' } as CSSProperties}
    >
      {src && !failed ? (
        <img
          ref={ref}
          src={src}
          alt={alt}
          loading={priority ? 'eager' : 'lazy'}
          decoding="async"
          draggable={false}
          className={loaded ? 'is-loaded' : undefined}
          onLoad={() => setLoaded(true)}
          onError={() => setFailed(true)}
        />
      ) : (
        <GeneratedArt seed={seed} title={title} subtitle={subtitle} size={size} />
      )}
    </div>
  );
}

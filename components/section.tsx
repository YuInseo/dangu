import Link from 'next/link';
import type { ReactNode } from 'react';
import { ChevronRight } from './icons';

export function Section({
  title,
  href,
  more = '모두 보기',
  children,
  sub,
}: {
  title: string;
  href?: string;
  more?: string;
  children: ReactNode;
  sub?: ReactNode;
}) {
  return (
    <section className="section">
      <div className="section-head">
        <h2 className="section-title">{title}</h2>
        {sub && <span className="section-sub">{sub}</span>}
        {href && (
          <Link href={href} className="section-more">
            {more}
            <ChevronRight size={16} />
          </Link>
        )}
      </div>
      {children}
    </section>
  );
}

export function PageHeader({ eyebrow, title, children }: { eyebrow?: string; title: string; children?: ReactNode }) {
  return (
    <header className="page-head">
      {eyebrow && <div className="eyebrow">{eyebrow}</div>}
      <h1 className="page-title">{title}</h1>
      {children}
    </header>
  );
}

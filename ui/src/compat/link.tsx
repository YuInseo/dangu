/**
 * Next의 `<Link href>`와 같은 모양으로 쓰는 링크. 컴포넌트를 거의 고치지 않고 옮기려고 둔다.
 */
import type { AnchorHTMLAttributes, ReactNode } from 'react';
import { Link as RouterLink } from 'react-router';

type Props = Omit<AnchorHTMLAttributes<HTMLAnchorElement>, 'href'> & {
  href: string;
  replace?: boolean;
  /** false면 이동해도 스크롤을 맨 위로 올리지 않는다. */
  scroll?: boolean;
  prefetch?: boolean;
  children?: ReactNode;
};

export default function Link({ href, replace, scroll, prefetch: _prefetch, ...rest }: Props) {
  if (/^(https?:)?\/\//.test(href) || href.startsWith('mailto:')) return <a href={href} {...rest} />;
  return <RouterLink to={href} replace={replace} preventScrollReset={scroll === false} {...rest} />;
}

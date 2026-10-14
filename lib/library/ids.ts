import { createHash } from 'node:crypto';

/** 짧고 URL에 안전한 고정 ID. 같은 입력이면 다시 스캔해도 같은 값이 나온다. */
export function makeId(input: string): string {
  return createHash('sha1').update(input).digest('base64url').slice(0, 12);
}

/** 비교용 정규화. 대소문자, 앞뒤 공백, 유니코드 조합 차이를 지운다. */
export function norm(s: string): string {
  return s.normalize('NFKC').trim().toLowerCase().replace(/\s+/g, ' ');
}

export function artistId(name: string): string {
  return makeId(`artist:${norm(name)}`);
}

export function genreId(name: string): string {
  return makeId(`genre:${norm(name)}`);
}

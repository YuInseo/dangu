import { callNative } from 'graft/native';

import { isNativeApp } from './platform';
import type { Kind, Need, SlipDraft } from './slip';

/**
 * 승차권을 찍어 쪽지를 채운다.
 *
 * 세 단계다: 사진(카메라나 앨범) → 글자(기기 안의 ML Kit, `TicketOcrPlugin.kt`) → 칸.
 * 마지막 단계가 이 파일의 대부분이다. 코레일 종이 승차권과 코레일톡·SRT 앱의 모바일
 * 승차권은 생김새가 달라도 들어 있는 것은 같다:
 *
 *   KTX 107   2026년 10월 12일
 *   서울 05:13 → 부산 07:52
 *   3호차 12A석
 *
 * 인식이 틀릴 수 있으므로 읽은 값을 바로 올리지 않는다. 쓰기 화면을 채워서 열어 주고,
 * 사람이 한 번 보고 올린다.
 */

/** 승차권에 찍히는 역들. 인식된 글자에서 역 이름을 골라낼 때 쓴다(긴 이름이 먼저 맞게). */
export const STATIONS = [
  '서울', '용산', '영등포', '광명', '수원', '행신', '청량리', '상봉', '수서', '동탄', '평택지제',
  '천안아산', '천안', '오송', '조치원', '대전', '서대전', '김천구미', '김천', '구미', '동대구', '대구',
  '서대구', '경산', '밀양', '물금', '구포', '부산', '신경주', '경주', '울산', '포항', '영천',
  '마산', '창원중앙', '창원', '진영', '진주', '공주', '계룡', '논산', '익산', '정읍', '광주송정',
  '광주', '나주', '목포', '전주', '남원', '곡성', '구례구', '순천', '여천', '여수엑스포',
  '만종', '횡성', '둔내', '평창', '진부', '강릉', '정동진', '묵호', '동해', '원주', '제천',
  '단양', '풍기', '영주', '안동', '의성', '청주', '청주공항', '홍성', '예산', '대천', '서천',
  '군산', '김제', '안양', '평택', '오산', '아산', '온양온천',
].sort((a, b) => b.length - a.length);

export interface TicketRead {
  draft: Partial<SlipDraft>;
  /** 무엇을 읽었는지 한 줄로. 쓰기 화면 위에 띄운다. */
  summary: string;
  /** 인식된 글자 전부. 아무것도 못 골라냈을 때 사람이 볼 수 있게. */
  raw: string;
}

/* 사진 ------------------------------------------------------------- */

/**
 * 표 사진을 받는다. 카메라로 찍거나 앨범(받은 캡처)에서 고른다.
 *
 * 앱에서는 Capacitor 카메라 플러그인, 웹에서는 파일 고르기. 취소하면 `null`.
 */
export async function pickTicketPhoto(): Promise<string | null> {
  const camera = (globalThis as any).Capacitor?.Plugins?.Camera;
  if (isNativeApp() && camera?.getPhoto) {
    try {
      const photo = await camera.getPhoto({
        resultType: 'base64',
        source: 'PROMPT',
        quality: 85,
        // 글자를 읽기에 충분하고, 인식기가 메모리에 올리기에 가벼운 크기.
        width: 1800,
        correctOrientation: true,
        promptLabelHeader: '승차권',
        promptLabelPhoto: '앨범에서 고르기',
        promptLabelPicture: '사진 찍기',
        promptLabelCancel: '취소',
      });
      return photo?.base64String ?? null;
    } catch {
      return null; // 취소
    }
  }

  // 웹: 파일 고르기. 폰 브라우저에서는 카메라도 고를 수 있다.
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'image/*';
    input.onchange = () => {
      const file = input.files?.[0];
      if (!file) return resolve(null);
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result));
      reader.onerror = () => resolve(null);
      reader.readAsDataURL(file);
    };
    input.click();
  });
}

/* 글자 ------------------------------------------------------------- */

interface OcrLine {
  text: string;
  top: number;
  left: number;
  height: number;
}

/** 사진에서 글자를 읽는다. 앱에서만 된다 — 인식기가 APK 안에 있다. */
export async function recognizeTicket(base64: string): Promise<string> {
  if (!isNativeApp()) throw new Error('글자 인식은 앱에서만 됩니다.');
  const result = await callNative<{ text: string; lines: OcrLine[] }>('TicketOcr', 'recognize', { base64 });
  return readingOrder(result.lines ?? []) || result.text || '';
}

/**
 * 줄들을 사람이 읽는 순서로 다시 잇는다.
 *
 * 인식기는 덩어리(block)별로 돌려주는데, 모바일 승차권은 "서울"과 "05:13"이 서로 다른
 * 덩어리에 떨어져 있는 경우가 많다. 같은 높이에 있는 것끼리 한 줄로 묶어야 "서울 05:13 →
 * 부산 07:52"가 한 줄이 된다.
 */
function readingOrder(lines: OcrLine[]): string {
  const sorted = [...lines].filter((line) => line.text?.trim()).sort((a, b) => a.top - b.top || a.left - b.left);
  const rows: OcrLine[][] = [];
  for (const line of sorted) {
    const row = rows[rows.length - 1];
    const anchor = row?.[0];
    if (anchor && Math.abs(line.top - anchor.top) < Math.max(anchor.height, line.height) * 0.6) row.push(line);
    else rows.push([line]);
  }
  return rows.map((row) => row.sort((a, b) => a.left - b.left).map((line) => line.text).join(' ')).join('\n');
}

/* 칸 --------------------------------------------------------------- */

const pad = (n: number) => String(n).padStart(2, '0');

/**
 * 인식된 글자를 쪽지 칸으로.
 *
 * `home`은 이 팀의 역들(팀 이름에서 찾는다 — "서울역 출도착"이면 서울). 표의 출발역이
 * 우리 역이면 출발 쪽지, 도착역이 우리 역이면 도착 쪽지가 된다. 모르면 `fallback`.
 */
export function parseTicket(text: string, home: string[], fallback: Kind): TicketRead {
  // 인식기가 자주 헷갈리는 것들을 먼저 고른다: 전각 콜론, 세미콜론으로 읽힌 콜론, 화살표 모양들.
  // 점(.)은 고치지 않는다 — "2026.10.12"의 점을 콜론으로 바꾸면 날짜가 시각이 된다.
  const clean = text
    .replace(/[：﹕]/g, ':')
    .replace(/(\d)\s*[:;]\s*(\d{2})(?!\d)/g, '$1:$2')
    .replace(/[➔➜→⇒▶►>]+/g, ' → ');

  const draft: Partial<SlipDraft> = {};
  const found: string[] = [];

  // 열차번호. "KTX 107", "KTX-산천 107", "SRT 301", "무궁화호 1203", "107열차".
  const train =
    /(?:KTX(?:[-\s]?(?:산천|이음|청룡))?|SRT|ITX[-\s]?(?:새마을|마음|청춘)?|새마을호?|무궁화호?|누리로)\s*[#.\-]?\s*(\d{1,4})/i.exec(
      clean
    )?.[1] ??
    /열차\s*번호\s*[:.]?\s*(\d{1,4})/.exec(clean)?.[1] ??
    /(\d{2,4})\s*열차/.exec(clean)?.[1];
  if (train) {
    draft.train = String(Number(train));
    found.push(`${draft.train}열차`);
  }

  // 호차·좌석. "3호차 12A석", "3 호차 12 A".
  const seat = /(\d{1,2})\s*호\s*차\D{0,4}?(\d{1,2})\s*([A-Da-d])/.exec(clean);
  if (seat) {
    draft.car = String(Number(seat[1]));
    draft.seat = `${Number(seat[2])}${seat[3].toUpperCase()}`;
    found.push(`${draft.car}-${draft.seat}`);
  } else {
    const car = /(\d{1,2})\s*호\s*차/.exec(clean);
    if (car) {
      draft.car = String(Number(car[1]));
      found.push(`${draft.car}호차`);
    }
  }

  // 날짜. "2026년 10월 12일", "2026.10.12".
  const date = /(20\d{2})\s*[.\-/년]\s*(\d{1,2})\s*[.\-/월]\s*(\d{1,2})/.exec(clean);
  if (date) draft.date = `${date[1]}-${pad(Number(date[2]))}-${pad(Number(date[3]))}`;

  // 역과 시각은 나오는 순서대로: 앞의 것이 출발, 뒤의 것이 도착.
  const stations: string[] = [];
  for (const match of clean.matchAll(new RegExp(STATIONS.join('|'), 'g'))) {
    if (!stations.includes(match[0])) stations.push(match[0]);
    if (stations.length === 2) break;
  }
  const times = [...clean.matchAll(/(?<!\d)([01]?\d|2[0-3]):([0-5]\d)(?!\d)/g)].map(
    (match) => `${pad(Number(match[1]))}:${match[2]}`
  );
  const [from, to] = stations;
  const [leaves, arrives] = times;

  // 우리 역이 어느 쪽인지로 출발·도착을 정한다.
  let kind: Kind = fallback;
  if (from && home.includes(from)) kind = 'depart';
  else if (to && home.includes(to)) kind = 'arrive';
  draft.kind = kind;

  if (kind === 'depart') {
    if (to) draft.station = to;
    if (leaves) draft.time = leaves;
    if (to === '행신') draft.haengsin = true;
  } else {
    if (from) draft.station = from;
    if (arrives ?? leaves) draft.time = arrives ?? leaves;
  }
  if (from || to) found.push(`${from ?? '?'} → ${to ?? '?'}`);
  if (draft.time) found.push(draft.time);

  // 휠체어석으로 끊은 표면 쪽지에도 휠필을 미리 켠다.
  if (/휠\s*체\s*어/.test(clean)) {
    draft.needs = ['wheelchair' as Need];
    found.push('휠체어석');
  }

  return {
    draft,
    summary: found.length ? `표에서 읽음: ${found.join(' · ')}` : '표에서 알아볼 수 있는 글자가 없었습니다.',
    raw: text,
  };
}

/** 팀 이름에 들어 있는 역들. "서울역 출도착" → ['서울']. */
export function homeStations(teamName: string): string[] {
  return STATIONS.filter((station) => teamName.includes(station));
}

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { buildSearchUrl, htmlToText, parseLength, parseSearch, parseShow, qualityFromFormats } from '../lib/live/archive';

// archive.org advancedsearch.php?output=json 응답의 모양
const search = {
  responseHeader: { status: 0 },
  response: {
    numFound: 2,
    start: 0,
    docs: [
      {
        identifier: 'gd1977-05-08.sbd.example.flac16',
        title: 'Grateful Dead Live at Barton Hall on 1977-05-08',
        creator: 'Grateful Dead',
        date: '1977-05-08T00:00:00Z',
        venue: 'Barton Hall, Cornell University',
        coverage: 'Ithaca, NY',
        downloads: 1234567,
        format: ['Flac', 'VBR MP3', 'Text', 'Checksums'],
      },
      {
        identifier: 'kglw2023-06-07.example',
        title: 'King Gizzard & The Lizard Wizard Live at Red Rocks',
        creator: ['King Gizzard & The Lizard Wizard'],
        date: '2023-06-07T00:00:00Z',
        format: ['24bit Flac', 'VBR MP3'],
      },
    ],
  },
};

// archive.org/metadata/{id} 응답의 모양
const show = {
  metadata: {
    identifier: 'kglw2023-06-07.example',
    title: 'King Gizzard & The Lizard Wizard Live at Red Rocks',
    creator: 'King Gizzard & The Lizard Wizard',
    date: '2023-06-07',
    venue: 'Red Rocks Amphitheatre',
    coverage: 'Morrison, CO',
    description: 'Set 1:<br />01. Mars For The Rich<br/>02. Hate Dancin&#39;<br><br>Recorded by &amp; transferred by taper',
    source: 'Schoeps MK4 > Sound Devices MixPre-6',
    lineage: 'WAV > FLAC',
    taper: 'Someone',
  },
  files: [
    { name: 'kglw2023-06-07d1t02.flac', format: '24bit Flac', source: 'original', title: "Hate Dancin'", track: '02', length: '5:12' },
    {
      name: 'kglw2023-06-07d1t01.flac',
      format: '24bit Flac',
      source: 'original',
      title: 'Mars For The Rich',
      track: '01',
      length: '423.54',
    },
    { name: 'kglw2023-06-07d1t01.mp3', format: 'VBR MP3', source: 'derivative', title: 'Mars For The Rich', length: '07:03' },
    { name: 'kglw2023-06-07.ffp', format: 'Flac FingerPrint', source: 'original' },
    { name: 'secret.flac', format: '24bit Flac', private: 'true' },
  ],
};

test('검색 결과', () => {
  const r = parseSearch(search);
  assert.equal(r.total, 2);
  assert.equal(r.shows[0]!.band, 'Grateful Dead');
  assert.equal(r.shows[0]!.date, '1977-05-08');
  assert.equal(r.shows[0]!.quality, 'flac');
  assert.equal(r.shows[1]!.band, 'King Gizzard & The Lizard Wizard');
  assert.equal(r.shows[1]!.quality, 'flac24');
});

test('검색 주소는 구절 검색이고 따옴표를 빼낸다', () => {
  const url = new URL(buildSearchUrl({ q: 'King "Gizzard"', sort: 'recent', page: 2 }));
  const q = url.searchParams.get('q')!;
  assert.match(q, /collection:etree/);
  assert.match(q, /creator:\("King  Gizzard"\)/);
  assert.equal(url.searchParams.get('sort[]'), 'publicdate desc');
  assert.equal(url.searchParams.get('page'), '2');
  assert.deepEqual(url.searchParams.getAll('fl[]').includes('format'), true);
});

test('공연: 가장 좋은 형식만, 트랙 번호 순서로, 비공개 파일은 빼고', () => {
  const s = parseShow('kglw2023-06-07.example', show)!;
  assert.equal(s.band, 'King Gizzard & The Lizard Wizard');
  assert.equal(s.tracks.length, 2);
  assert.deepEqual(
    s.tracks.map((t) => t.title),
    ['Mars For The Rich', "Hate Dancin'"],
  );
  assert.equal(s.tracks[0]!.format.bitDepth, 24);
  assert.equal(s.tracks[0]!.format.lossless, true);
  assert.equal(Math.round(s.tracks[0]!.duration), 424);
  assert.equal(s.tracks[1]!.duration, 312);
  assert.equal(s.tracks[0]!.url, 'https://archive.org/download/kglw2023-06-07.example/kglw2023-06-07d1t01.flac');
  assert.match(s.notes!, /01\. Mars For The Rich\n02\. Hate Dancin'/);
  assert.match(s.notes!, /Recorded by & transferred/);
});

test('FLAC이 없으면 MP3로', () => {
  const s = parseShow('x', {
    metadata: { title: 'x', creator: 'Band' },
    files: [{ name: 'a.mp3', format: 'VBR MP3', title: 'A', bitrate: '218' }],
  })!;
  assert.equal(s.tracks[0]!.format.codec, 'MP3');
  assert.equal(s.tracks[0]!.format.lossless, false);
  assert.equal(s.tracks[0]!.format.bitrate, 218000);
});

test('없는 공연', () => {
  assert.equal(parseShow('x', {}), null);
});

test('길이와 글', () => {
  assert.equal(parseLength('1:02:03'), 3723);
  assert.equal(parseLength('05:43'), 343);
  assert.equal(parseLength('343.5'), 343.5);
  assert.equal(parseLength(undefined), 0);
  assert.equal(htmlToText('<p>a</p><p>b &amp; c</p>'), 'a\nb & c');
  assert.equal(qualityFromFormats(['Shorten', 'Ogg Vorbis']), 'lossy');
});

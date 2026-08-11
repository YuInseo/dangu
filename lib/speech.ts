import { plugin } from './platform';

/**
 * 소리 내어 읽기.
 *
 * 큐를 들고 있는 사람은 화면을 보고 있지 않다. 한 점을 올린 뒤 몇 점이 남았는지는
 * 그때 제일 알고 싶은 것이면서 화면에서만 볼 수 있는 값이라, 이 앱에서 소리가 붙을
 * 자리가 있다면 거기다.
 *
 * 길은 둘이고 둘 다 있어야 한다.
 *
 * 안드로이드 WebView에는 `speechSynthesis`가 없다. 크롬에는 있어서 브라우저로 열면
 * 그대로 들리지만, 앱 안에서는 같은 코드가 조용히 아무 일도 하지 않는다 — 그래서 셸에
 * TextToSpeech 플러그인을 얹고 그쪽을 먼저 본다. 웹에서는 플러그인이 없으므로
 * `speechSynthesis`로 떨어진다. 둘 다 없으면 아무 일도 일어나지 않는다: 소리는 이
 * 앱에서 없으면 아쉬운 것이지 없으면 안 되는 것이 아니다.
 */

const isBrowser = () => typeof window !== 'undefined';

/**
 * 지금 읽고 있는 것을 끊고 새로 읽는다.
 *
 * 점수는 연달아 눌린다 — 한 큐에 세 점을 넣으면 세 번이 순식간에 쌓인다. 큐에 넣어
 * 차례로 읽으면 이미 지난 점수를 계속 읽고 있게 되므로, 마지막 것만 남긴다.
 */
export function speak(text: string): void {
  if (!isBrowser() || !text) return;

  const native = plugin('TextToSpeech');
  if (native?.speak) {
    void (async () => {
      try {
        await native.stop?.();
        await native.speak({ text, lang: 'ko-KR', rate: 1.1, pitch: 1, volume: 1 });
      } catch {
        // 기기에 한국어 TTS가 없거나 엔진이 아직 안 떴을 때. 점수판은 계속 돈다.
      }
    })();
    return;
  }

  try {
    const synth = (window as any).speechSynthesis;
    if (!synth) return;
    synth.cancel();
    const utterance = new (window as any).SpeechSynthesisUtterance(text);
    utterance.lang = 'ko-KR';
    utterance.rate = 1.1;
    synth.speak(utterance);
  } catch {
    // 브라우저가 막아 둔 경우.
  }
}

/** 읽던 것을 멈춘다. 게임을 떠날 때 부른다 — 로비에서 점수가 들리면 곤란하다. */
export function hush(): void {
  if (!isBrowser()) return;
  const native = plugin('TextToSpeech');
  if (native?.stop) {
    void native.stop().catch(() => {});
    return;
  }
  try {
    (window as any).speechSynthesis?.cancel();
  } catch {
    // 없으면 멈출 것도 없다.
  }
}

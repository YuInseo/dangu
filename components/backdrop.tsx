import { artColors } from '@/lib/hash';

/**
 * 상세 화면 뒤에 까는 흐린 표지. 룬의 앨범 화면이 표지 색에 물드는 그 느낌.
 * 표지가 없으면 생성 그림과 같은 색으로 물들인다.
 */
export function Backdrop({ src, color, seed }: { src?: string; color?: string; seed?: string }) {
  let background: string | undefined;
  if (!src && seed) {
    const { a, b } = artColors(seed);
    background = `radial-gradient(60% 80% at 20% 0%, hsl(${a} 55% 40%), transparent 70%), radial-gradient(50% 70% at 80% 10%, hsl(${b} 60% 45%), transparent 70%)`;
  }
  return (
    <div className="backdrop" aria-hidden style={{ backgroundColor: color, backgroundImage: background }}>
      {src && <img src={src} alt="" />}
    </div>
  );
}

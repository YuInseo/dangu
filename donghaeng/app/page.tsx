import { Home } from './home';

export const metadata = { title: '동행' };

/** 첫 화면. 껍데기만 그리고, 무엇을 보여 줄지는 기기에 있는 이름과 팀이 정한다. */
export default function HomePage() {
  return <Home />;
}

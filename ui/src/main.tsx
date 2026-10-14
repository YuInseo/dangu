import 'pretendard/dist/web/variable/pretendardvariable-dynamic-subset.css';
import './styles.css';

import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { Outlet, RouterProvider, ScrollRestoration, createBrowserRouter } from 'react-router';
import { Shell } from '@/components/shell';
import { ToastProvider } from '@/components/toast';
import { DspProvider } from '@/lib/dsp/store';
import { PlayerProvider } from '@/lib/player/context';
import { LibraryProvider, useLibraryInfo } from '@/library';
import { AlbumPage } from '@/pages/album';
import { AlbumsPage } from '@/pages/albums';
import { ArtistPage } from '@/pages/artist';
import { ArtistsPage } from '@/pages/artists';
import { DspScreen } from '@/pages/dsp';
import { GenrePage } from '@/pages/genre';
import { GenresPage } from '@/pages/genres';
import { Home } from '@/pages/home';
import { LivePage } from '@/pages/live';
import { LiveShowPage } from '@/pages/live-show';
import { LoginPage } from '@/pages/login';
import { NotFound } from '@/pages/not-found';
import { SearchPage } from '@/pages/search';
import { AboutSettings, AudioSettings, GeneralSettings, LibrarySettings, PlaySettings, SettingsLayout } from '@/pages/settings';
import { TracksPage } from '@/pages/tracks';

function Main() {
  const info = useLibraryInfo();
  return (
    <ToastProvider>
      <DspProvider>
        <PlayerProvider canTranscode caps={{ soxr: true, bs2b: true }}>
          <Shell status={info.status} summary={info.summary}>
            <Outlet />
          </Shell>
          <ScrollRestoration />
        </PlayerProvider>
      </DspProvider>
    </ToastProvider>
  );
}

function Root() {
  return (
    <LibraryProvider fallback={<div className="boot" aria-busy="true" />}>
      <Main />
    </LibraryProvider>
  );
}

const router = createBrowserRouter([
  { path: '/login', element: <LoginPage /> },
  {
    element: <Root />,
    children: [
      { index: true, element: <Home /> },
      { path: 'albums', element: <AlbumsPage /> },
      { path: 'albums/:id', element: <AlbumPage /> },
      { path: 'artists', element: <ArtistsPage /> },
      { path: 'artists/:id', element: <ArtistPage /> },
      { path: 'tracks', element: <TracksPage /> },
      { path: 'genres', element: <GenresPage /> },
      { path: 'genres/:id', element: <GenrePage /> },
      { path: 'search', element: <SearchPage /> },
      { path: 'live', element: <LivePage /> },
      { path: 'live/:id', element: <LiveShowPage /> },
      { path: 'dsp', element: <DspScreen /> },
      {
        path: 'settings',
        element: <SettingsLayout />,
        children: [
          { index: true, element: <GeneralSettings /> },
          { path: 'play', element: <PlaySettings /> },
          { path: 'library', element: <LibrarySettings /> },
          { path: 'audio', element: <AudioSettings /> },
          { path: 'about', element: <AboutSettings /> },
        ],
      },
      { path: '*', element: <NotFound /> },
    ],
  },
]);

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <RouterProvider router={router} />
  </StrictMode>,
);

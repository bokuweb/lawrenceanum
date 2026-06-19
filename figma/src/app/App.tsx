import { Suspense, lazy } from "react";
import { HashRouter, Navigate, Route, Routes, useLocation, useNavigate, useParams, useSearchParams } from "react-router";
import { ThemeProvider } from "./components/theme-provider";
import { SidebarNav } from "./components/sidebar-nav";
import { Topbar } from "./components/topbar";

// 各ビューを遅延ロード。
const DashboardView = lazy(() => import("./components/views/dashboard-view").then(m => ({ default: m.DashboardView })));
const SearchView = lazy(() => import("./components/views/search-view").then(m => ({ default: m.SearchView })));
const BrowseView = lazy(() => import("./components/views/browse-view").then(m => ({ default: m.BrowseView })));
const CompareView = lazy(() => import("./components/views/compare-view").then(m => ({ default: m.CompareView })));
const UpdatesView = lazy(() => import("./components/views/simple-views").then(m => ({ default: m.UpdatesView })));
const KanpoView = lazy(() => import("./components/views/simple-views").then(m => ({ default: m.KanpoView })));
const SettingsView = lazy(() => import("./components/views/simple-views").then(m => ({ default: m.SettingsView })));
const ProceedingsView = lazy(() => import("./components/views/proceedings-view").then(m => ({ default: m.ProceedingsView })));
const PubcommentView = lazy(() => import("./components/views/pubcomment-view").then(m => ({ default: m.PubcommentView })));
const FeedView = lazy(() => import("./components/views/feed-view").then(m => ({ default: m.FeedView })));
const EnforcementView = lazy(() => import("./components/views/enforcement-view").then(m => ({ default: m.EnforcementView })));
const GianView = lazy(() => import("./components/views/gian-view").then(m => ({ default: m.GianView })));

function ViewFallback() {
  return <div className="p-6 text-sm text-muted-foreground">読み込み中…</div>;
}

function SearchRoute() {
  const [params, setParams] = useSearchParams();
  const navigate = useNavigate();
  return (
    <SearchView
      initialQuery={params.get("q") ?? ""}
      onQueryChange={(q) => {
        if (q) setParams({ q }, { replace: true });
        else setParams({}, { replace: true });
      }}
      onOpen={(l) => navigate(`/laws/${l.law_id}`)}
    />
  );
}

function BrowseRoute() {
  const { lawId } = useParams();
  const navigate = useNavigate();
  return (
    <BrowseView
      lawId={lawId ?? null}
      onSelect={(id) => navigate(id ? `/laws/${id}` : "/laws")}
      onCompare={(id) => navigate(`/laws/${id}/compare`)}
    />
  );
}

function CompareRoute() {
  const { lawId } = useParams();
  return <CompareView initialLawId={lawId ?? null} />;
}

function ProceedingsRoute() {
  const { meetingId } = useParams();
  const navigate = useNavigate();
  return (
    <ProceedingsView
      meetingId={meetingId ?? null}
      onSelectMeeting={(id) => navigate(id ? `/proceedings/${id}` : "/proceedings")}
    />
  );
}

function PubcommentRoute() {
  const { caseId } = useParams();
  const navigate = useNavigate();
  return (
    <PubcommentView
      caseId={caseId ?? null}
      onSelectCase={(id) => navigate(id ? `/pubcomment/${id}` : "/pubcomment")}
    />
  );
}

function GianRoute() {
  const { session, billId } = useParams();
  const navigate = useNavigate();
  return (
    <GianView
      billRef={session && billId ? { session, billId } : null}
      onSelect={(s, id) => navigate(`/gian/${s}/${id}`)}
    />
  );
}

function AppShell() {
  const navigate = useNavigate();
  const location = useLocation();
  const [params, setParams] = useSearchParams();
  // /search にいるときは URL の ?q= と双方向同期、それ以外のページでは入力即 /search?q= に遷移する。
  const onSearchPage = location.pathname === "/search";
  const topbarValue = onSearchPage ? params.get("q") ?? "" : "";
  return (
    <div className="size-full flex bg-background text-foreground">
      <SidebarNav />
      <div className="flex-1 flex flex-col min-w-0">
        <Topbar
          value={topbarValue}
          onChange={(q) => {
            if (onSearchPage) {
              if (q) setParams({ q }, { replace: true });
              else setParams({}, { replace: true });
            } else {
              const url = q ? `/search?q=${encodeURIComponent(q)}` : "/search";
              navigate(url, { replace: true });
            }
          }}
        />
        <main className="flex-1 overflow-auto min-h-0">
          <Suspense fallback={<ViewFallback />}>
            <Routes>
              <Route path="/" element={<DashboardView />} />
              <Route path="/search" element={<SearchRoute />} />
              <Route path="/laws" element={<BrowseRoute />} />
              <Route path="/laws/:lawId" element={<BrowseRoute />} />
              <Route path="/laws/:lawId/compare" element={<CompareRoute />} />
              <Route path="/compare" element={<CompareRoute />} />
              <Route path="/proceedings" element={<ProceedingsRoute />} />
              <Route path="/proceedings/:meetingId" element={<ProceedingsRoute />} />
              <Route path="/pubcomment" element={<PubcommentRoute />} />
              <Route path="/pubcomment/:caseId" element={<PubcommentRoute />} />
              <Route path="/feed" element={<FeedView />} />
              <Route path="/gian" element={<GianRoute />} />
              <Route path="/gian/:session/:billId" element={<GianRoute />} />
              <Route path="/enforcement" element={<EnforcementView />} />
              <Route path="/updates" element={<UpdatesView />} />
              <Route path="/kanpo" element={<KanpoView />} />
              <Route path="/settings" element={<SettingsView />} />
              <Route path="*" element={<Navigate to="/" replace />} />
            </Routes>
          </Suspense>
        </main>
      </div>
    </div>
  );
}

export default function App() {
  // HashRouter を採用: GitHub Pages の SPA fallback (404.html) なしで deep link が動く。
  // 静的 JSON API (`/laws/*.json`) は通常パス、UI のルーティングは `/#/...` 側を使う。
  return (
    <ThemeProvider>
      <HashRouter>
        <AppShell />
      </HashRouter>
    </ThemeProvider>
  );
}

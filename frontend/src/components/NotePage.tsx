import NoteContent from './NoteContent';
import BacklinksSidebar from './BacklinksSidebar';
import CitationFooter from './CitationFooter';

export default function NotePage() {
  return (
    <div className="min-h-screen bg-gray-100 flex">
      {/* Main centered paper */}
      <main className="flex-1 flex items-center justify-center p-8">
        <div className="bg-white shadow-lg rounded-lg p-8 max-w-2xl w-full">
          <NoteContent />
        </div>
      </main>
      
      {/* Side panel for backlinks & citation */}
      <aside className="w-80 bg-white border-l border-gray-200 flex flex-col">
        <div className="flex-1 p-6">
          <BacklinksSidebar />
        </div>
        <div className="border-t border-gray-200">
          <CitationFooter />
        </div>
      </aside>
    </div>
  );
}
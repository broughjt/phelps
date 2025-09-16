export default function CitationFooter() {
  return (
    <div className="bg-gray-50 p-4">
      <div className="space-y-3">
        <h3 className="text-sm font-medium text-gray-900">
          Citation
        </h3>
        
        <div className="space-y-2 text-xs text-gray-600">
          <p><span className="font-medium">Created:</span> Sep 16, 2025</p>
          <p><span className="font-medium">Modified:</span> Sep 16, 2025</p>
          <p><span className="font-medium">Source:</span> Personal KB</p>
          <p><span className="font-medium">ID:</span> note-001</p>
        </div>
        
        <div className="flex gap-2 pt-2">
          <button className="text-xs text-blue-600 hover:text-blue-800 transition-colors">
            Export
          </button>
          <button className="text-xs text-blue-600 hover:text-blue-800 transition-colors">
            Share
          </button>
        </div>
      </div>
    </div>
  );
}
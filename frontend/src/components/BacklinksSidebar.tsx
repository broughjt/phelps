export default function BacklinksSidebar() {
  const mockBacklinks = [
    { id: 1, title: "Related Concept A", preview: "This note explores similar themes..." },
    { id: 2, title: "Foundation Theory", preview: "The underlying principles that..." },
    { id: 3, title: "Advanced Applications", preview: "Building upon this concept..." },
    { id: 4, title: "Historical Context", preview: "The development of these ideas..." },
    { id: 5, title: "Cross-Reference Study", preview: "Connections between different..." },
  ];

  return (
    <div className="h-full overflow-y-auto">
      <h2 className="text-lg font-semibold text-gray-900 mb-4">
        Backlinks
      </h2>
      
      <div className="space-y-3">
        {mockBacklinks.map((backlink) => (
          <div 
            key={backlink.id}
            className="border border-gray-200 rounded-lg p-3 hover:bg-gray-50 cursor-pointer transition-colors"
          >
            <h3 className="font-medium text-sm text-blue-600 hover:text-blue-800 mb-1">
              {backlink.title}
            </h3>
            <p className="text-xs text-gray-600 line-clamp-2">
              {backlink.preview}
            </p>
          </div>
        ))}
      </div>
      
      <div className="mt-6 pt-4 border-t border-gray-200">
        <p className="text-xs text-gray-500">
          {mockBacklinks.length} notes link to this page
        </p>
      </div>
    </div>
  );
}
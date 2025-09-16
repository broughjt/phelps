export default function NoteContent() {
  return (
    <article className="max-w-2xl mx-auto prose prose-lg">
      <h1 className="text-3xl font-bold text-gray-900 mb-6">
        Sample Note Title
      </h1>
      
      <div className="space-y-4 text-gray-700 leading-relaxed">
        <p>
          Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod 
          tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, 
          quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.
        </p>
        
        <p>
          Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore 
          eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, 
          sunt in culpa qui officia deserunt mollit anim id est laborum.
        </p>
        
        <blockquote className="border-l-4 border-blue-500 pl-4 italic text-gray-600">
          "Sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium 
          doloremque laudantium, totam rem aperiam."
        </blockquote>
        
        <p>
          At vero eos et accusamus et iusto odio dignissimos ducimus qui blanditiis 
          praesentium voluptatum deleniti atque corrupti quos dolores et quas molestias 
          excepturi sint occaecati cupiditate non provident.
        </p>
        
        <ul className="list-disc pl-6 space-y-2">
          <li>First important point to remember</li>
          <li>Second key insight from this note</li>
          <li>Third connection to other concepts</li>
        </ul>
      </div>
    </article>
  );
}
import "./NotePage.css";

      // <aside>
      //   <ul>
      //     <li>Backlink 1</li>
      //     <li>Backlink 1</li>
      //     <li>Backlink 1</li>
      //   </ul>
      // </aside>

export default function NotePage() {
  return (
    <div className="layout">
      <article>
        <h1>Tommy the cat</h1>
        
        <p>Well I remember it as though it were a meal ago…</p>
        
        <p>
             Said Tommy the Cat as he reeled back to clear whatever foreign matter may have
             nestled its way into his mighty throat. Many a fat alley rat had met its
             demise while staring point blank down the cavernous barrel of this awesome
             prowling machine. Truly a wonder of nature this urban predator — Tommy the cat
             had many a story to tell. But it was a rare occasion such as this that he did.
        </p>

        Here is a list.
        <ul>
          <li>First list item</li>
          <li>Second list item</li>
          <li>Third list item</li>
        </ul>
      </article>
      <aside>
        <p>Yo boss, there are 3 baclinks</p>
        <ul>
          <li>Backlink 1</li>
          <li>Backlink 2</li>
          <li>Backlink 3</li>
        </ul>
      </aside>
      <div className="right-column"></div>
    </div>
  )
}

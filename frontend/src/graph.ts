export class Graph {
  outgoing: Record<string, Set<string>>;
  incoming: Record<string, Set<string>>;

  static fromOutgoing(outgoing: Record<string, string[]>): Graph {
    const graph = new Graph();
    graph.outgoing = Object.fromEntries(
      Object.entries(outgoing).map(([key, value]) => [key, new Set(value)]),
    );
    graph.incoming = Object.fromEntries(
      Object.keys(outgoing).map((key) => [key, new Set()]),
    );

    for (const [i, js] of Object.entries(outgoing)) {
      for (const j of js) {
        // Ensure `j` exists in incoming map in case it wasn't a key in `outgoing`
        if (!graph.incoming[j]) {
          graph.incoming[j] = new Set();
        }
        graph.incoming[j].add(i);
      }
    }

    return graph;
  }

  shallowCopy(): Graph {
    const newGraph = new Graph();
    newGraph.outgoing = { ...this.outgoing };
    newGraph.incoming = { ...this.incoming };
    return newGraph;
  }

  constructor() {
    this.outgoing = {};
    this.incoming = {};
  }

  addNode(i: string) {
    if (!this.outgoing[i]) {
      this.outgoing[i] = new Set();
      this.incoming[i] = new Set();
    }
  }

  addEdge(i: string, j: string) {
    this.addNode(i);
    this.addNode(j);
    this.outgoing[i].add(j);
    this.incoming[j].add(i);
  }

  removeEdge(i: string, j: string) {
    this.outgoing[i]?.delete(j);
    this.incoming[j]?.delete(i);
  }

  removeNode(i: string) {
    if (!this.outgoing[i]) return;
    for (const j of this.outgoing[i]) {
      this.incoming[j]?.delete(i);
    }
    for (const j of this.incoming[i]) {
      this.outgoing[j]?.delete(i);
    }
    delete this.outgoing[i];
    delete this.incoming[i];
  }

  hasNode(i: string) {
    return this.outgoing[i] !== undefined;
  }
}

export type Graph = {
  outgoing: Map<string, Set<string>>;
  incoming: Map<string, Set<string>>;
};

export function updateNodeEdges(
  graph: Graph,
  node: string,
  newOut: Set<string>,
  newIn: Set<string>,
): Graph {
  const { outgoing, incoming } = graph;

  const newOutgoing = new Map(outgoing);
  const newIncoming = new Map(incoming);

  const oldOut = new Set(outgoing.get(node) ?? []);
  const oldIn = new Set(incoming.get(node) ?? []);

  for (const destination of oldOut) {
    if (!newOut.has(destination)) {
      const updated = new Set(newIncoming.get(destination) ?? []);
      updated.delete(node);
      newIncoming.set(destination, updated);
    }
  }
  for (const destination of newOut) {
    if (!oldOut.has(destination)) {
      const updated = new Set(newIncoming.get(destination) ?? []);
      updated.add(node);
      newIncoming.set(destination, updated);
    }
  }
  newOutgoing.set(node, new Set(newOut));

  for (const source of oldIn) {
    if (!newIn.has(source)) {
      const updated = new Set(newOutgoing.get(source) ?? []);
      updated.delete(node);
      newOutgoing.set(source, updated);
    }
  }
  for (const source of newIn) {
    if (!oldIn.has(source)) {
      const updated = new Set(newOutgoing.get(source) ?? []);
      updated.add(node);
      newOutgoing.set(source, updated);
    }
  }
  newIncoming.set(node, new Set(newIn));

  return { outgoing: newOutgoing, incoming: newIncoming };
}

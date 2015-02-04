#include "fallbackedgedb.h"

#include "../dfs.h"
#include "../exactannokeysearch.h"

#include <fstream>
#include <limits>

using namespace annis;
using namespace std;

FallbackEdgeDB::FallbackEdgeDB(StringStorage &strings, const Component &component)
  : strings(strings), component(component)
{
}

void FallbackEdgeDB::copy(const DB &db, const ReadableGraphStorage &orig)
{
  clear();

  ExactAnnoKeySearch nodes(db, annis_ns, annis_node_name);
  while(nodes.hasNext())
  {
    nodeid_t source = nodes.next().node;
    std::vector<nodeid_t> outEdges = orig.getOutgoingEdges(source);
    for(auto target : outEdges)
    {
      Edge e = {source, target};
      addEdge(e);
      std::vector<Annotation> edgeAnnos = orig.getEdgeAnnotations(e);
      for(auto a : edgeAnnos)
      {
        addEdgeAnnotation(e, a);
      }
    }
  }

  stat = orig.getStatistics();

  calculateIndex();
}

void FallbackEdgeDB::addEdge(const Edge &edge)
{
  if(edge.source != edge.target)
  {
    edges.insert(edge);
    stat.valid = false;
  }
}

void FallbackEdgeDB::addEdgeAnnotation(const Edge& edge, const Annotation &anno)
{
  edgeAnnos.addEdgeAnnotation(edge, anno);
}

void FallbackEdgeDB::clear()
{
  edges.clear();
  edgeAnnos.clear();

  stat.valid = false;
}

bool FallbackEdgeDB::isConnected(const Edge &edge, unsigned int minDistance, unsigned int maxDistance) const
{
  typedef stx::btree_set<Edge>::const_iterator EdgeIt;
  if(minDistance == 1 && maxDistance == 1)
  {
    EdgeIt it = edges.find(edge);
    if(it != edges.end())
    {
      return true;
    }
    else
    {
      return false;
    }
  }
  else
  {
    CycleSafeDFS dfs(*this, edge.source, minDistance, maxDistance);
    DFSIteratorResult result = dfs.nextDFS();
    while(result.found)
    {
      if(result.node == edge.target)
      {
        return true;
      }
      result = dfs.nextDFS();
    }
  }

  return false;
}

std::unique_ptr<EdgeIterator> FallbackEdgeDB::findConnected(nodeid_t sourceNode,
                                                 unsigned int minDistance,
                                                 unsigned int maxDistance) const
{
  return std::unique_ptr<EdgeIterator>(
        new UniqueDFS(*this, sourceNode, minDistance, maxDistance));
}

int FallbackEdgeDB::distance(const Edge &edge) const
{
  CycleSafeDFS dfs(*this, edge.source, 0, uintmax);
  DFSIteratorResult result = dfs.nextDFS();
  while(result.found)
  {
    if(result.node == edge.target)
    {
      return result.distance;
    }
    result = dfs.nextDFS();
  }
  return -1;
}

std::vector<Annotation> FallbackEdgeDB::getEdgeAnnotations(const Edge& edge) const
{
  return edgeAnnos.getEdgeAnnotations(edge);
}

std::vector<nodeid_t> FallbackEdgeDB::getOutgoingEdges(nodeid_t node) const
{
  typedef stx::btree_set<Edge>::const_iterator EdgeIt;

  vector<nodeid_t> result;

  EdgeIt lowerIt = edges.lower_bound(Init::initEdge(node, numeric_limits<uint32_t>::min()));
  EdgeIt upperIt = edges.upper_bound(Init::initEdge(node, numeric_limits<uint32_t>::max()));

  for(EdgeIt it = lowerIt; it != upperIt; it++)
  {
    result.push_back(it->target);
  }

  return result;
}

bool FallbackEdgeDB::load(std::string dirPath)
{
  clear();

  ReadableGraphStorage::load(dirPath);

  ifstream in;

  in.open(dirPath + "/edges.btree");
  edges.restore(in);
  in.close();

  edgeAnnos.load(dirPath);

  return true;

}

bool FallbackEdgeDB::save(std::string dirPath)
{
  ReadableGraphStorage::save(dirPath);

  ofstream out;

  out.open(dirPath + "/edges.btree");
  edges.dump(out);
  out.close();

  edgeAnnos.save(dirPath);

  return true;
}

std::uint32_t FallbackEdgeDB::numberOfEdges() const
{
  return edges.size();
}

std::uint32_t FallbackEdgeDB::numberOfEdgeAnnotations() const
{
  return edgeAnnos.numberOfEdgeAnnotations();
}

void FallbackEdgeDB::calculateStatistics()
{
  stat.valid = false;
  stat.maxFanOut = 0;
  stat.maxDepth = 1;
  stat.avgFanOut = 0.0;
  stat.cyclic = false;
  stat.rootedTree = true;
  stat.nodes = 0;

  unsigned int sumFanOut = 0;


  std::unordered_set<nodeid_t> hasIncomingEdge;

  // find all root nodes
  unordered_set<nodeid_t> roots;
  unordered_set<nodeid_t> allNodes;
  for(const auto& e : edges)
  {
    roots.insert(e.source);
    allNodes.insert(e.source);
    allNodes.insert(e.target);

    if(stat.rootedTree)
    {
      auto findTarget = hasIncomingEdge.find(e.target);
      if(findTarget == hasIncomingEdge.end())
      {
        hasIncomingEdge.insert(e.target);
      }
      else
      {
        stat.rootedTree = false;
      }
    }
  }

  stat.nodes = allNodes.size();
  allNodes.clear();

  auto itFirstEdge = edges.begin();
  if(itFirstEdge != edges.end())
  {
    nodeid_t lastSourceID = itFirstEdge->source;
    uint32_t currentFanout = 0;

    for(const auto& e : edges)
    {
      roots.erase(e.target);

      if(lastSourceID != e.source)
      {

        stat.maxFanOut = std::max(stat.maxFanOut, currentFanout);
        sumFanOut += currentFanout;

        currentFanout = 0;
        lastSourceID = e.source;
      }
      currentFanout++;
    }
    // add the statistics for the last node
    stat.maxFanOut = std::max(stat.maxFanOut, currentFanout);
    sumFanOut += currentFanout;
  }


  uint64_t numberOfVisits = 0;
  if(roots.empty() && !edges.empty())
  {
    // if we have edges but no roots at all there must be a cycle
    stat.cyclic = true;
  }
  else
  {
    for(const auto& rootNode : roots)
    {
      CycleSafeDFS dfs(*this, rootNode, 0, uintmax, false);
      for(auto n = dfs.nextDFS(); n.found; n = dfs.nextDFS())
      {
        numberOfVisits++;


        stat.maxDepth = std::max(stat.maxDepth, n.distance);
      }
      if(dfs.cyclic())
      {
        stat.cyclic = true;
      }
    }
  }

  if(stat.cyclic)
  {
    // it's infinite
    stat.maxDepth = 0;
    stat.dfsVisitRatio = 0.0;
  }
  else
  {
    if(stat.nodes > 0)
    {
      stat.dfsVisitRatio = (double) numberOfVisits / (double) stat.nodes;
    }
  }

  if(sumFanOut > 0 && stat.nodes > 0)
  {
    stat.avgFanOut =  (double) sumFanOut / (double) stat.nodes;
  }

  stat.valid = true;

}

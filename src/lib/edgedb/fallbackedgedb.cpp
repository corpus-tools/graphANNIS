#include "fallbackedgedb.h"

using namespace annis;

FallbackEdgeDB::FallbackEdgeDB(const Component &component)
  : component(component)
{
}

void FallbackEdgeDB::addEdge(Edge edge)
{
  edges.insert2(edge.first, edge.second);
}

void FallbackEdgeDB::addEdgeAnnotation(Edge edge, const Annotation &anno)
{
  edgeAnnotations.insert2(edge, anno);
}

void FallbackEdgeDB::clear()
{
  edges.clear();
  edgeAnnotations.clear();
}

const Component &FallbackEdgeDB::getComponent()
{
  return component;
}

bool FallbackEdgeDB::isConnected(const Edge &edge, unsigned int distance)
{
  if(distance == 0)
  {
    return false;
  }
  else if(distance == 1)
  {
    stx::btree_map<std::uint32_t, std::uint32_t>::const_iterator it
        = edges.find(edge.first);
    if(it == edges.end())
    {
      return false;
    }
    else
    {
      return it->second == edge.second;
    }
  }
  else
  {
    throw("Not implemented yet");
  }
}

std::vector<Annotation> FallbackEdgeDB::getEdgeAnnotations(Edge edge)
{
  typedef stx::btree_multimap<Edge, Annotation>::const_iterator ItType;

  std::vector<Annotation> result;

  std::pair<ItType, ItType> range =
      edgeAnnotations.equal_range(edge);

  for(ItType it=range.first; it != range.second; ++it)
  {
    result.push_back(it->second);
  }

  return result;
}

#include "fallbackedgedb.h"

#include <fstream>

using namespace annis;
using namespace std;

FallbackEdgeDB::FallbackEdgeDB(StringStorage &strings, const Component &component)
  : strings(strings), component(component)
{
}

void FallbackEdgeDB::addEdge(const Edge &edge)
{
  if(edge.source != edge.target)
  {
    edges.insert2(edge.source, edge.target);
  }
}

void FallbackEdgeDB::addEdgeAnnotation(const Edge& edge, const Annotation &anno)
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

bool FallbackEdgeDB::isConnected(const Edge &edge, unsigned int distance) const
{
  typedef stx::btree_multimap<uint32_t, uint32_t>::const_iterator EdgeIt;
  if(distance == 0)
  {
    return false;
  }
  else if(distance == 1)
  {
    pair<EdgeIt, EdgeIt> range = edges.equal_range(edge.source);
    for(EdgeIt it = range.first; it != range.second; it++)
    {
      if(it->second == edge.target)
      {
        return true;
      }
    }
    return false;
  }
  else
  {
    throw("Not implemented yet");
  }
}

AnnotationIterator *FallbackEdgeDB::findConnected(std::uint32_t sourceNode,
                                                 unsigned int minDistance,
                                                 unsigned int maxDistance) const
{
  return new FallbackReachableIterator(*this, sourceNode, minDistance, maxDistance);
}

std::vector<Annotation> FallbackEdgeDB::getEdgeAnnotations(const Edge& edge)
{
  typedef stx::btree_multimap<Edge, Annotation, compEdges>::const_iterator ItType;

  std::vector<Annotation> result;

  std::pair<ItType, ItType> range =
      edgeAnnotations.equal_range(edge);

  for(ItType it=range.first; it != range.second; ++it)
  {
    result.push_back(it->second);
  }

  return result;
}

bool FallbackEdgeDB::load(std::string dirPath)
{
  clear();

  ifstream in;

  in.open(dirPath + "/edges.btree");
  edges.restore(in);
  in.close();

  in.open(dirPath + "/edgeAnnotations.btree");
  edgeAnnotations.restore(in);
  in.close();

  return true;

}

bool FallbackEdgeDB::save(std::string dirPath)
{
  ofstream out;

  out.open(dirPath + "/edges.btree");
  edges.dump(out);
  out.close();

  out.open(dirPath + "/edgeAnnotations.btree");
  edgeAnnotations.dump(out);
  out.close();

  return true;
}

std::uint32_t FallbackEdgeDB::numberOfEdges() const
{
  return edges.size();
}

std::uint32_t FallbackEdgeDB::numberOfEdgeAnnotations() const
{
  return edgeAnnotations.size();
}

FallbackReachableIterator::FallbackReachableIterator(const FallbackEdgeDB &edb,
                                                     std::uint32_t startNode,
                                                     unsigned int minDistance,
                                                     unsigned int maxDistance)
  : edb(edb), minDistance(minDistance), maxDistance(maxDistance)
{
  EdgeIt it = edb.edges.find(startNode);
  if(it != edb.edges.end())
  {
    traversalStack.push(it);
  }
}

bool FallbackReachableIterator::hasNext()
{
  return !traversalStack.empty();
}

Match FallbackReachableIterator::next()
{
  Match result;
  if(!traversalStack.empty())
  {
    EdgeIt it = traversalStack.top();
    traversalStack.pop();

    // get the next node
    result.first = it->second;
    result.second.name = STRING_STORAGE_ANY;
    result.second.ns = STRING_STORAGE_ANY;
    result.second.val = STRING_STORAGE_ANY;

    // update iterator and add it to the stack again if there are more siblings
    it++;
    if(it != edb.edges.end())
    {
      traversalStack.push(it);
    }


  }
  return result;
}

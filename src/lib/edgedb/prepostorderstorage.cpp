#include "prepostorderstorage.h"

#include <set>
#include <stack>

#include <fstream>
#include <boost/archive/binary_oarchive.hpp>
#include <boost/archive/binary_iarchive.hpp>
#include <boost/serialization/map.hpp>


using namespace annis;

PrePostOrderStorage::PrePostOrderStorage(StringStorage &strings, const Component &component)
 : FallbackEdgeDB(strings, component)
{

}

PrePostOrderStorage::~PrePostOrderStorage()
{

}

bool PrePostOrderStorage::load(std::string dirPath)
{
  node2order.clear();
  order2node.clear();

  bool result = FallbackEdgeDB::load(dirPath);
  std::ifstream in;

  in.open(dirPath + "/node2order.btree", std::ios::binary);
  result = result && node2order.restore(in);
  in.close();

  in.open(dirPath + "/order2node.btree", std::ios::binary);
  result = result && order2node.restore(in);
  in.close();

  return result;
}

bool PrePostOrderStorage::save(std::string dirPath)
{
  bool result = FallbackEdgeDB::save(dirPath);

  std::ofstream out;

  out.open(dirPath + "/node2order.btree", std::ios::binary);
  node2order.dump(out);
  out.close();

  out.open(dirPath + "/order2node.btree", std::ios::binary);
  order2node.dump(out);
  out.close();

  return result;

}

void PrePostOrderStorage::calculateIndex()
{
  using ItType = stx::btree_set<Edge>::const_iterator;
  node2order.clear();
  order2node.clear();

  // find all roots of the component
  std::set<nodeid_t> roots;
  // first add all nodes that are a source of an edge as possible roots
  for(ItType it = getEdgesBegin(); it != getEdgesEnd(); it++)
  {
    roots.insert(it->source);
  }
  // second delete the ones that have an outgoing edge
  for(ItType it = getEdgesBegin(); it != getEdgesEnd(); it++)
  {
    roots.erase(it->target);
  }

  // traverse the graph for each sub-component
  for(const auto& startNode : roots)
  {
    unsigned int lastDistance = 0;

    uint32_t currentOrder = 0;
    std::stack<nodeid_t> nodeStack;

    enterNode(currentOrder, startNode, startNode, 0, nodeStack);

    FallbackDFSIterator dfs(*this, startNode, 1, uintmax);
    for(DFSIteratorResult step = dfs.nextDFS(); step.found;
          step = dfs.nextDFS())
    {
      if(step.distance > lastDistance)
      {
        // first visited, set pre-order
        enterNode(currentOrder, step.node, startNode, step.distance, nodeStack);
      }
      else if(step.distance == lastDistance)
      {
        // neighbour node, the last subtree was iterated completly, thus the last node
        // can be assigned a post-order
        exitNode(currentOrder, nodeStack, startNode);

        // new node
        enterNode(currentOrder, step.node, startNode, step.distance, nodeStack);
      }
      else
      {
        // parent node, the subtree was iterated completly, thus the last node
        // can be assigned a post-order
        exitNode(currentOrder, nodeStack, startNode);

        // the current node was already visited
      }
      lastDistance = step.distance;
    } // end for each DFS step

    while(!nodeStack.empty())
    {
      exitNode(currentOrder, nodeStack, startNode);
    }

  } // end for each root
}

void PrePostOrderStorage::enterNode(uint32_t& currentOrder, nodeid_t nodeID, nodeid_t rootNode,
                                        int32_t level, std::stack<nodeid_t>& nodeStack)
{
  order2node[currentOrder] = nodeID;
  PrePost newEntry;
  newEntry.pre = currentOrder++;
  newEntry.level = level;
  newEntry.rootNode = rootNode;
  node2order.insert2(nodeID, newEntry);
  nodeStack.push(nodeID);
}

void PrePostOrderStorage::exitNode(uint32_t& currentOrder, std::stack<nodeid_t>& nodeStack, uint32_t rootNode)
{
  order2node[currentOrder] = nodeStack.top();
  // find the correct pre/post entry and update the post-value
  auto itPair = node2order.equal_range(nodeStack.top());
  for(auto& it=itPair.first; it != itPair.second; it++)
  {
    if(it->second.rootNode == rootNode)
    {
      it->second.post = currentOrder++;
      break;
    }
  }
  nodeStack.pop();
}


bool PrePostOrderStorage::isConnected(const Edge &edge, unsigned int minDistance, unsigned int maxDistance)
{
  const auto& orderSource = node2order.find(edge.source);
  const auto& orderTarget = node2order.find(edge.target);
  if(orderSource != node2order.end() && orderTarget != node2order.end())
  {
    if(orderSource->second.pre <= orderTarget->second.pre
       && orderTarget->second.post <= orderSource->second.post)
    {
      // check the level
      int32_t diffLevel = (orderTarget->second.level - orderSource->second.level);
      return minDistance <= diffLevel <= maxDistance;
    }
  }
  return false;
}


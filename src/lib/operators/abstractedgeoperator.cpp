#include <annis/operators/abstractedgeoperator.h>

#include <annis/wrapper.h>
#include <stx/btree_set>

using namespace annis;

AbstractEdgeOperator::AbstractEdgeOperator(
    ComponentType componentType,
    const DB &db, std::string ns, std::string name,
    unsigned int minDistance, unsigned int maxDistance)
  : componentType(componentType), db(db), ns(ns), name(name),
                  minDistance(minDistance), maxDistance(maxDistance),
                  anyAnno(Init::initAnnotation()), edgeAnno(anyAnno)
{
  initGraphStorage();
}

AbstractEdgeOperator::AbstractEdgeOperator(
    ComponentType componentType,
    const DB &db, std::string ns, std::string name, const Annotation &edgeAnno)
  : componentType(componentType), db(db), ns(ns), name(name),
    minDistance(1), maxDistance(1),
    anyAnno(Init::initAnnotation()), edgeAnno(edgeAnno)
{
  initGraphStorage();
}

std::unique_ptr<AnnoIt> AbstractEdgeOperator::retrieveMatches(const Match &lhs)
{
  ListWrapper* w = new ListWrapper();


  // add the rhs nodes of all of the edge storages
  if(gs.size() == 1)
  {
    std::unique_ptr<EdgeIterator> it = gs[0]->findConnected(lhs.node, minDistance, maxDistance);
    for(auto m = it->next(); m.first; m = it->next())
    {
      if(checkEdgeAnnotation(gs[0], lhs.node, m.second))
      {
        // directly add the matched node since when having only one component
        // no duplicates are possible
        w->addMatch(m.second);
      }
    }
  }
  else if(gs.size() > 1)
  {
    stx::btree_set<nodeid_t> uniqueResult;
    for(auto e : gs)
    {
      std::unique_ptr<EdgeIterator> it = e->findConnected(lhs.node, minDistance, maxDistance);
      for(auto m = it->next(); m.first; m = it->next())
      {
        if(checkEdgeAnnotation(e, lhs.node, m.second))
        {
          uniqueResult.insert(m.second);
        }
      }
    }
    for(const auto& n : uniqueResult)
    {
      w->addMatch(n);
    }
  }
  return std::unique_ptr<AnnoIt>(w);
}

bool AbstractEdgeOperator::filter(const Match &lhs, const Match &rhs)
{
  // check if the two nodes are connected in *any* of the edge storages
  for(auto e : gs)
  {
    if(e->isConnected(Init::initEdge(lhs.node, rhs.node), minDistance, maxDistance))
    {
      if(checkEdgeAnnotation(e, lhs.node, rhs.node))
      {
        return true;
      }
    }
  }
  return false;
}


void AbstractEdgeOperator::initGraphStorage()
{
  if(ns == "")
  {
    gs = db.getGraphStorage(componentType, name);
  }
  else
  {
    // directly add the only known edge storage
    const ReadableGraphStorage* e = db.getGraphStorage(componentType, ns, name);
    if(e != nullptr)
    {
      gs.push_back(e);
    }
  }
}

bool AbstractEdgeOperator::checkEdgeAnnotation(const ReadableGraphStorage* e, nodeid_t source, nodeid_t target)
{
  if(edgeAnno == anyAnno)
  {
    return true;
  }
  else
  {
    // check if the edge has the correct annotation first
    auto edgeAnnoList = e->getEdgeAnnotations(Init::initEdge(source, target));
    for(const auto& anno : edgeAnnoList)
    {
      if(checkAnnotationEqual(edgeAnno, anno))
      {
        return true;
      }
    } // end for each annotation of candidate edge
  }
  return false;
}

AbstractEdgeOperator::~AbstractEdgeOperator()
{

}


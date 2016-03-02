#include <annis/query.h>
#include <annis/join/nestedloop.h>
#include <annis/join/seed.h>
#include <annis/filter.h>
#include <annis/operators/operator.h>
#include <annis/db.h>
#include <annis/iterators.h>
#include <annis/annosearch/annotationsearch.h>
#include <annis/wrapper.h>

#include <vector>
#include <re2/re2.h>

using namespace annis;

Query::Query(const DB &db)
  : db(db)
{
}

Query::~Query() {
  
}

size_t annis::Query::addNode(std::shared_ptr<AnnotationSearch> n, bool wrapAnyNodeAnno)
{
  bestPlan.reset();

  size_t idx = nodes.size();
  
  if(wrapAnyNodeAnno)
  {
    Annotation constAnno = {db.getNodeNameStringID(), db.getNamespaceStringID(), 0};
    nodes.push_back(std::make_shared<ConstAnnoWrapper>(constAnno, n));
  }
  else
  {
    nodes.push_back(n);
  }
  return idx;
}

size_t annis::Query::addNode(std::shared_ptr<AnnotationKeySearch> n, bool wrapAnyNodeAnno)
{
  bestPlan.reset();

  size_t idx = nodes.size();
  if(wrapAnyNodeAnno)
  {
    Annotation constAnno = {db.getNodeNameStringID(), db.getNamespaceStringID(), 0};
    nodes.push_back(std::make_shared<ConstAnnoWrapper>(constAnno, n));
  }
  else
  {
    nodes.push_back(n);
  }
  return idx;
}

void Query::addOperator(std::shared_ptr<Operator> op, size_t idxLeft, size_t idxRight, bool useNestedLoop)
{
  bestPlan.reset();

  OperatorEntry entry;
  entry.op = op;
  entry.useNestedLoop = useNestedLoop;
  entry.idxLeft = idxLeft;
  entry.idxRight = idxRight;
  
  operators.push_back(entry);
}

void Query::optimize()
{
  if(!bestPlan && db.nodeAnnos.hasStatistics())
  {
    // for each commutative operator check if is better to switch the operands
    for(auto& e : operators)
    {
      if(e.op && e.op->isCommutative() && e.idxLeft < nodes.size() && e.idxRight < nodes.size())
      {
        std::shared_ptr<EstimatedSearch> lhs = 
          std::dynamic_pointer_cast<EstimatedSearch>(nodes[e.idxLeft]);
        std::shared_ptr<EstimatedSearch> rhs = 
          std::dynamic_pointer_cast<EstimatedSearch>(nodes[e.idxRight]);
        
        if(lhs && rhs)
        {
          std::int64_t estimateLHS = lhs->guessMaxCount();
          std::int64_t estimateRHS = rhs->guessMaxCount();
          
          if(estimateLHS >= 0 && estimateRHS >= 0 && estimateLHS > estimateRHS)
          {
            // the left one is larger, so switch both operands
            size_t oldLeft = e.idxLeft;
            e.idxLeft = e.idxRight;
            e.idxRight = oldLeft;
          }

        }
      }
    }
    
    // TODO: optimize join order
  }
}

std::shared_ptr<Plan> Query::createPlan(const std::vector<std::shared_ptr<AnnoIt> >& nodes, const std::list<OperatorEntry>& operators, const DB& db) 
{
  std::vector<std::shared_ptr<AnnoIt>> source;
  std::map<int, int> querynode2component;
  
  // 1. add all nodes
  int i=0;
  for(auto& n : nodes)
  {
    source.push_back(n);
    querynode2component[i]=i;
    i++;
  }
  
  // 2. add the operators which produce the results
  for(auto& e : operators)
  {
    if(e.idxLeft < source.size() && e.idxRight < source.size())
    {
      int leftComponent = querynode2component[e.idxLeft];
      int rightComponent = querynode2component[e.idxRight];

      if(leftComponent == rightComponent)
      {
        addJoin(source, db, e, true);
      }
      else
      {
        addJoin(source, db, e, false);
        mergeComponents(querynode2component, leftComponent, rightComponent);
      }
    }
  }
  
   // 3. check if every node is connected
  int firstComponent;
  bool firstComponentSet = false;
  for(const auto& e : querynode2component)
  {
    if(firstComponentSet)
    {
      if(e.second != firstComponent)
      {
        std::cerr << "Node " << e.first << " is not connected" << std::endl;
        return std::shared_ptr<Plan>();
      }
    }
    else
    {
      firstComponent = e.second;
      firstComponentSet = true;
    }
  }
  return std::make_shared<Plan>(source);
}



void Query::internalInit()
{
  if(bestPlan) {
    return;
  }

  bestPlan = createPlan(nodes, operators, db);
  currentResult.resize(nodes.size());
}

void Query::addJoin(std::vector<std::shared_ptr<AnnoIt>>& source, const DB& db, const OperatorEntry& e, bool filterOnly)
{
  std::shared_ptr<Iterator> j;
  if(filterOnly)
  {
    j = std::make_shared<Filter>(e.op, source[e.idxLeft], source[e.idxRight]);
  }
  else
  {
    if(e.useNestedLoop)
    {
      j = std::make_shared<NestedLoopJoin>(e.op, source[e.idxLeft], source[e.idxRight]);
    }
    else
    {
      std::shared_ptr<AnnoIt> rightIt = source[e.idxRight];
      
      std::shared_ptr<ConstAnnoWrapper> constWrapper =
          std::dynamic_pointer_cast<ConstAnnoWrapper>(rightIt);
      if(constWrapper)
      {
        rightIt = constWrapper->getDelegate();
      }
      
      std::shared_ptr<AnnotationKeySearch> keySearch =
          std::dynamic_pointer_cast<AnnotationKeySearch>(rightIt);
      std::shared_ptr<AnnotationSearch> annoSearch =
          std::dynamic_pointer_cast<AnnotationSearch>(rightIt);

      if(keySearch)
      {
        j = std::make_shared<AnnoKeySeedJoin>(db, e.op, source[e.idxLeft],
            keySearch->getValidAnnotationKeys());
      }
      else if(annoSearch)
      {
        j = std::make_shared<MaterializedSeedJoin>(db, e.op, source[e.idxLeft],
            annoSearch->getValidAnnotations());
      }
      else
      {
        // fallback to nested loop
        j = std::make_shared<NestedLoopJoin>(e.op, source[e.idxLeft], source[e.idxRight]);
      }
    }
  }

  std::shared_ptr<JoinWrapIterator> itLeft =
      std::make_shared<JoinWrapIterator>(j, true);
  std::shared_ptr<JoinWrapIterator> itRight =
      std::make_shared<JoinWrapIterator>(j, false);

  itLeft->setOther(itRight);
  itRight->setOther(itLeft);
  
  source[e.idxLeft] = itLeft;
  source[e.idxRight] = itRight;
}

void Query::mergeComponents(std::map<int, int>& querynode2component, int c1, int c2)
{
  if(c1 == c2)
  {
    // nothing todo
    return;
  }

  std::vector<int> nodeIDsForC2;
  for(const auto e : querynode2component)
  {
    if(e.second == c2)
    {
      nodeIDsForC2.push_back(e.first);
    }
  }
  // set the component id for each node of the other component
  for(auto nodeID : nodeIDsForC2)
  {
    querynode2component[nodeID] = c1;
  }
}


bool Query::next()
{
  if(!bestPlan)
  {
    internalInit();
  }
  
  return bestPlan->executeStep(currentResult);
}




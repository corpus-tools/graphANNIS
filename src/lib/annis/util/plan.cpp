/*
 * To change this license header, choose License Headers in Project Properties.
 * To change this template file, choose Tools | Templates
 * and open the template in the editor.
 */

/* 
 * File:   Plan.cpp
 * Author: thomas
 * 
 * Created on 1. März 2016, 11:48
 */

#include <annis/util/plan.h>
#include <map>
#include <memory>

#include <annis/db.h>
#include <annis/wrapper.h>
#include <annis/operators/operator.h>
#include <annis/join/nestedloop.h>
#include <annis/join/seed.h>
#include <annis/join/taskindexjoin.h>
#include <annis/join/indexjoin.h>
#include <annis/filter.h>

using namespace annis;

Plan::Plan(std::shared_ptr<ExecutionNode> root)
  : root(root)
{
}

Plan::Plan(const Plan& orig)
{
  root = orig.root;
}

std::shared_ptr<ExecutionNode> Plan::join(std::shared_ptr<Operator> op,
    size_t lhsNode, size_t rhsNode,
    std::shared_ptr<ExecutionNode> lhs, std::shared_ptr<ExecutionNode> rhs,
    const DB& db,
    bool forceNestedLoop,
    QueryConfig config)
{
  
  ExecutionNodeType type = ExecutionNodeType::nested_loop;
  if(lhs->componentNr == rhs->componentNr)
  {
    type = ExecutionNodeType::filter;
  }
  else if(rhs->type == ExecutionNodeType::base && !forceNestedLoop)
  { 
    // if the right side is not another join we can use a seed join
    type = ExecutionNodeType::seed;
  }
  else if(config.avoidNestedBySwitch && !forceNestedLoop
    && op->isCommutative()
    && lhs->type == ExecutionNodeType::base)
  {
    // avoid a nested loop join by switching the operands
    std::shared_ptr<ExecutionNode> tmp = lhs;
    lhs = rhs;
    rhs = tmp;
    
    size_t tmpNodeID = lhsNode;
    lhsNode = rhsNode;
    rhsNode = tmpNodeID;
    
    type = ExecutionNodeType::seed;
  }
  
  std::shared_ptr<ExecutionNode> result = std::make_shared<ExecutionNode>();
  auto mappedPosLHS = lhs->nodePos.find(lhsNode);
  auto mappedPosRHS = rhs->nodePos.find(rhsNode);
  
  // make sure both source nodes are contained in the previous execution nodes
  if(mappedPosLHS == lhs->nodePos.end() || mappedPosRHS == rhs->nodePos.end())
  {
    // TODO: throw error?
    return result;
  }
  
  // create the join iterator
  
  std::shared_ptr<Iterator> join;
  if(type == ExecutionNodeType::filter)
  {
    result->type = ExecutionNodeType::filter;
    join = std::make_shared<Filter>(op, lhs->join, mappedPosLHS->second, mappedPosRHS->second);
  }
  else if(type == ExecutionNodeType::seed)
  {
    result->type = ExecutionNodeType::seed;
      
    std::shared_ptr<Iterator> rightIt = rhs->join;

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
      if(config.threadPool)
      {
        join = std::make_shared<TaskIndexJoin>(lhs->join, mappedPosLHS->second, op,
                                               createAnnotationKeySearchFilter(db, keySearch), 128, config.threadPool);
      }
      else if(config.nonParallelJoinImpl == NonParallelJoin::seed)
      {
        join = std::make_shared<AnnoKeySeedJoin>(db, op, lhs->join,
          mappedPosLHS->second,
          keySearch->getValidAnnotationKeys());
      }
      else
      {
        join = std::make_shared<IndexJoin>(db, op, lhs->join,
                                           mappedPosLHS->second,
                                           createAnnotationKeySearchFilter(db, keySearch));
      }
    }
    else if(annoSearch)
    {
      if(config.threadPool)
      {
        join = std::make_shared<TaskIndexJoin>(lhs->join, mappedPosLHS->second, op,
                                               createAnnotationSearchFilter(db, annoSearch), 128, config.threadPool);
      }
      else if(config.nonParallelJoinImpl == NonParallelJoin::seed)
      {
        join = std::make_shared<MaterializedSeedJoin>(db, op, lhs->join,
          mappedPosLHS->second,
              annoSearch->getValidAnnotations());
      }
      else
      {
        join = std::make_shared<IndexJoin>(db, op, lhs->join,
                                           mappedPosLHS->second,
                                           createAnnotationSearchFilter(db, annoSearch));
      }

    }
    else
    {
      // fallback to nested loop
      result->type = nested_loop;
      join = std::make_shared<NestedLoopJoin>(op, lhs->join, rhs->join,
                                              mappedPosLHS->second, mappedPosRHS->second, true, true);
    }
  }
  else
  {
    result->type = ExecutionNodeType::nested_loop;
    
    auto leftEst = estimateTupleSize(lhs);
    auto rightEst = estimateTupleSize(rhs);
    
    bool leftIsOuter = leftEst->output <= rightEst->output;
    
    join = std::make_shared<NestedLoopJoin>(op, lhs->join, rhs->join,
                                            mappedPosLHS->second, mappedPosRHS->second, true, leftIsOuter);
  }
  
  result->join = join;
  result->op = op;
  result->componentNr = lhs->componentNr;
  result->lhs = lhs;
  result->description =  "#" + std::to_string(lhsNode+1) + " " 
    + op->description() + " #" + std::to_string(rhsNode+1);
  
  if(type != ExecutionNodeType::filter)
  {
    // only set a rhs when this is an actual join
    result->rhs = rhs;
  }
  rhs->componentNr = result->componentNr;
  
  // merge both node positions
  for(const auto& pos : lhs->nodePos)
  {
    result->nodePos.insert(pos);
  }
  // the RHS has an offset after the join
  size_t offset = lhs->nodePos.size();
  for(const auto& pos : rhs->nodePos)
  {
    result->nodePos.insert({pos.first, pos.second + offset});
  }
  
  return result;
}


bool Plan::executeStep(std::vector<Match>& result)
{
  if(root && root->join)
  {
    return root->join->next(result);
  }
  else
  {
    return false;
  }
}

double Plan::getCost() 
{
  // the estimation is cached in the root so multiple calls to getCost() won't do any harm
  return static_cast<double>(estimateTupleSize(root)->intermediateSum);
}

  std::shared_ptr<ExecutionEstimate> Plan::estimateTupleSize(std::shared_ptr<ExecutionNode> node)
{
  static const std::uint64_t defaultBaseTuples = 100000;
  static const double defaultSelectivity = 0.1;
  if(node)
  {
    if (node->estimate)
    {
      // return the cached estimate
      return node->estimate;
    } 
    else
    {
      std::shared_ptr<EstimatedSearch> baseEstimate =
        std::dynamic_pointer_cast<EstimatedSearch>(node->join);
      if (baseEstimate)
      {
        // directly use the estimated search this exec node
        std::int64_t guess = baseEstimate->guessMaxCount();
        if (guess >= 0)
        {
          node->estimate = std::make_shared<ExecutionEstimate>((std::uint64_t) guess, 0);
          return node->estimate;
        } 
        else
        {
          node->estimate = std::make_shared<ExecutionEstimate>(defaultBaseTuples, 0);
          return node->estimate;
        }
      } 
      else if (node->lhs && node->rhs)
      {
        // this is a join node, the estimated number of of tuple is
        // (count(lhs) * count(rhs)) * selectivity(op)
        auto estLHS = estimateTupleSize(node->lhs);
        auto estRHS = estimateTupleSize(node->rhs);
        double selectivity = defaultSelectivity;
        long double operatorSelectivity = defaultSelectivity;
        if(node->op)
        {
          selectivity = operatorSelectivity = node->op->selectivity();
          double edgeAnnoSelectivity = node->op->edgeAnnoSelectivity();
          if(edgeAnnoSelectivity >= 0.0)
          {
            selectivity = selectivity * edgeAnnoSelectivity;
          }
        }
        
        std::uint64_t outputSize = static_cast<std::uint64_t>(((long double) estLHS->output) * ((long double) estRHS->output) * ((long double) selectivity));
        if(outputSize < 1)
        {
          // always assume at least one output item otherwise very small selectivity can fool the planner
          outputSize = 1;
        }
        std::uint64_t processedInStep;

        if (node->type == ExecutionNodeType::nested_loop)
        {
          if(estLHS->output < estRHS->output)
          {
            // we use LHS as outer
            processedInStep = estLHS->output + (estLHS->output * estRHS->output);
          }
          else
          {
            // we use RHS as outer
            processedInStep = estRHS->output + (estRHS->output * estLHS->output);
          }
        } 
        else if (node->type == ExecutionNodeType::seed)
        {
          // A index join processes each LHS and for each LHS the number of reachable nodes given by the operator.
          // The selectivity of the operator itself an estimation how many nodes are filtered out by the cross product.
          // We can use this number (without the edge annotation selectivity) to re-construct the number of reachable nodes.

          // avgReachable = (sel * cross) / lhs
          //              = (sel * lhs * rhs) / lhs
          //              = sel * rhs
          // processedInStep = lhs + (avgReachable * lhs)
          //                 = lhs + (sel * rhs * lhs)


          processedInStep =
              static_cast<std::uint64_t>(
                (long double) estLHS->output
                + (operatorSelectivity * (long double) estRHS->output * (long double) estLHS->output)
              );
        } 
        else
        {
          processedInStep = estLHS->output;
        }

        // return the output of this node and the sum of all intermediate results
        node->estimate = 
          std::make_shared<ExecutionEstimate>(outputSize, processedInStep + estLHS->intermediateSum + estRHS->intermediateSum);
        return node->estimate;

      }
      else if (node->lhs)
      { 
        // this is a filter node, the estimated number of of tuple is
        // count(lhs) * selectivity(op)
        auto estLHS = estimateTupleSize(node->lhs);
        double selectivity = defaultSelectivity;
        if(node->op)
        {
          selectivity = node->op->selectivity();
        }

        std::uint64_t processedInStep = estLHS->output;
        std::uint64_t outputSize = static_cast<std::uint64_t>(((double) estLHS->output) * selectivity);
       
        // return the output of this node and the sum of all intermediate results
        node->estimate = 
          std::make_shared<ExecutionEstimate>(outputSize, processedInStep + estLHS->intermediateSum);
        return node->estimate;

      }
    } // end if no cached estimate given
  }
  else
  {
    // a non-existing node doesn't have any cost
    node->estimate =  std::make_shared<ExecutionEstimate>(0, 0);
    return node->estimate;
  }
  
  // we don't know anything about this node, return some large estimate
  // TODO: use DB do get a number relative to the overall number of nodes/annotations
  node->estimate = std::make_shared<ExecutionEstimate>(defaultBaseTuples, defaultBaseTuples);
  return node->estimate;
}

bool Plan::hasNestedLoop() const 
{
  return descendendantHasNestedLoop(root);
}

std::function<std::list<Match> (nodeid_t)> Plan::createSearchFilter(const DB &db, std::shared_ptr<EstimatedSearch> search)
{
  std::shared_ptr<ConstAnnoWrapper> constWrapper = std::dynamic_pointer_cast<ConstAnnoWrapper>(search);
  boost::optional<Annotation> constAnno;
  if(constWrapper)
  {
    search = constWrapper->getDelegate();
    constAnno = constWrapper->getConstAnno();
  }

  std::shared_ptr<AnnotationSearch> annoSearch = std::dynamic_pointer_cast<AnnotationSearch>(search);
  if(annoSearch)
  {
    return createAnnotationSearchFilter(db, annoSearch, constAnno);
  }
  std::shared_ptr<AnnotationKeySearch> annoKeySearch = std::dynamic_pointer_cast<AnnotationKeySearch>(search);
  if(annoKeySearch)
  {
    return createAnnotationKeySearchFilter(db, annoKeySearch, constAnno);
  }

  return [](nodeid_t) -> std::list<Match>  {return std::list<Match>();};
}

bool Plan::descendendantHasNestedLoop(std::shared_ptr<ExecutionNode> node)
{
  if(node)
  {
    if(node->type == ExecutionNodeType::nested_loop)
    {
      return true;
    }
    
    if(node->lhs) 
    {
      if(descendendantHasNestedLoop(node->lhs))
      {
        return true;
      }
    }
    
    if(node->rhs) 
    {
      if(descendendantHasNestedLoop(node->rhs))
      {
        return true;
      }
    }
  }
  return false;
}

std::function<std::list<Match> (nodeid_t)> Plan::createAnnotationSearchFilter(
    const DB& db,
    std::shared_ptr<AnnotationSearch> annoSearch, boost::optional<Annotation> constAnno)
{
  const std::unordered_set<Annotation>& validAnnos = annoSearch->getValidAnnotations();
  if(validAnnos.size() == 1)
  {
    const auto& rightAnno = *(validAnnos.begin());

    // no further checks required
    return [&db, rightAnno, constAnno](nodeid_t rhsNode) -> std::list<Match>
    {
      std::list<Match> result;
      auto foundAnno =
          db.nodeAnnos.getAnnotations(rhsNode, rightAnno.ns, rightAnno.name);

      if(!foundAnno.empty() && foundAnno[0].val == rightAnno.val)
      {
        if(constAnno)
        {
          result.push_back({rhsNode, *constAnno});
        }
        else
        {
          result.push_back({rhsNode, foundAnno[0]});
        }
      }

      return std::move(result);
    };
  }
  else
  {
    return [&db, validAnnos, constAnno](nodeid_t rhsNode) -> std::list<Match>
    {
      std::list<Match> result;
      // check all annotations which of them matches
      std::vector<Annotation> annos = db.nodeAnnos.getAnnotations(rhsNode);
      for(const auto& a : annos)
      {
        if(validAnnos.find(a) != validAnnos.end())
        {
          if(constAnno)
          {
            result.push_back({rhsNode, *constAnno});
          }
          else
          {
            result.push_back({rhsNode, a});
          }
        }
      }

      return std::move(result);
    };
  }
}


std::function<std::list<Match> (nodeid_t)> Plan::createAnnotationKeySearchFilter(const DB& db,
    std::shared_ptr<AnnotationKeySearch> annoKeySearch, boost::optional<Annotation> constAnno)
{
  const std::set<AnnotationKey>& validAnnoKeys = annoKeySearch->getValidAnnotationKeys();
  if(validAnnoKeys.size() == 1)
  {
    const auto& rightAnnoKey = *(validAnnoKeys.begin());

    // no further checks required
    return [&db, rightAnnoKey, constAnno](nodeid_t rhsNode) -> std::list<Match>
    {
      std::list<Match> result;
      auto foundAnno =
          db.nodeAnnos.getAnnotations(rhsNode, rightAnnoKey.ns, rightAnnoKey.name);

      if(!foundAnno.empty())
      {
        if(constAnno)
        {
          result.push_back({rhsNode, *constAnno});
        }
        else
        {
          result.push_back({rhsNode, foundAnno[0]});
        }

      }

      return result;
    };
  }
  else
  {
    return [&db, validAnnoKeys, constAnno](nodeid_t rhsNode) -> std::list<Match>
    {
      std::list<Match> result;
      // check all annotation keys
      for(AnnotationKey key : validAnnoKeys)
      {
       auto found = db.nodeAnnos.getAnnotations(rhsNode, key.ns, key.name);
       if(!found.empty())
       {
         if(constAnno)
         {
           result.push_back({rhsNode, *constAnno});
         }
         else
         {
          result.push_back({rhsNode, found[0]});
         }
       }
      }
      return result;
    };
  }
}


void Plan::clearCachedEstimate(std::shared_ptr<ExecutionNode> node) 
{
  if(node)
  {
    node->estimate.reset();
    
    if(node->lhs)
    {
      clearCachedEstimate(node->lhs);
    }

    if(node->rhs)
    {
      clearCachedEstimate(node->rhs);
    }
  }
}

std::string Plan::debugString() const
{
  return debugStringForNode(root, "");
}

std::string Plan::debugStringForNode(std::shared_ptr<const ExecutionNode> node, std::string indention) const
{
  if(!node)
  {
    return "";
  }
  
  std::string result = indention + "(";
  
  if(node->type == ExecutionNodeType::base)
  {
    // output the node number
    result += "#" + std::to_string(node->nodePos.begin()->first + 1);
    std::shared_ptr<EstimatedSearch> annoSearch = std::dynamic_pointer_cast<EstimatedSearch>(node->join);
    if(annoSearch)
    {
      std::string annoDebugString = annoSearch->debugString();
      if(!annoDebugString.empty())
      {
        result += ": " + annoDebugString;
      }
    }
  }
  else
  {
    result += typeToString(node->type);
  }
  result += ")";
  
  if(!node->description.empty())
  {
    result += "(" + node->description + ")";
  }
  
  if(node->estimate)
  {
    result +=  "[out: " 
      + std::to_string((std::uint64_t) node->estimate->output) 
      + " sum: " 
      + std::to_string((std::uint64_t) node->estimate->intermediateSum) 
      + "]";
  }
  if(node->op)
  {
    result += "{sel: " + std::to_string(node->op->selectivity()) + "}";
  }
  
  result += "\n";
  
  if(node->lhs)
  {
    result += debugStringForNode(node->lhs, indention + "    ");
  }
  if(node->rhs)
  {
    result += debugStringForNode(node->rhs, indention + "    ");
  }
  
  return result;
}

std::string Plan::typeToString(ExecutionNodeType type) const
{
  switch(type)
  {
    case ExecutionNodeType::base:
      return "base";
    case ExecutionNodeType::filter:
      return "filter";
    case ExecutionNodeType::nested_loop:
      return "nested_loop";
    case ExecutionNodeType::seed:
      return "seed";
    default:
      return "<unknown>";
  }

}


Plan::~Plan()
{
}


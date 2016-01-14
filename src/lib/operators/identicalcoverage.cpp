/* 
 * File:   IdenticalCoverage.cpp
 * Author: thomas
 * 
 * Created on 8. Januar 2016, 13:58
 */

#include <annis/operators/identicalcoverage.h>
#include <annis/wrapper.h>

#include <set>
#include <vector>
#include <algorithm>

using namespace annis;

IdenticalCoverage::IdenticalCoverage(const DB &db)
: db(db), tokHelper(db),
  anyNodeAnno(Init::initAnnotation(db.getNodeNameStringID(), 0, db.getNamespaceStringID()))
{
  gsOrder = db.getGraphStorage(ComponentType::ORDERING, annis_ns, "");
  gsLeftToken = db.getGraphStorage(ComponentType::LEFT_TOKEN, annis_ns, "");
  gsRightToken = db.getGraphStorage(ComponentType::RIGHT_TOKEN, annis_ns, "");
}

bool IdenticalCoverage::filter(const Match& lhs, const Match& rhs)
{
  return tokHelper.leftTokenForNode(lhs.node) == tokHelper.leftTokenForNode(rhs.node)
    && tokHelper.rightTokenForNode(lhs.node) == tokHelper.rightTokenForNode(rhs.node);
}

std::unique_ptr<AnnoIt> IdenticalCoverage::retrieveMatches(const Match& lhs)
{
  
  
  nodeid_t leftToken;
  nodeid_t rightToken;
  if(tokHelper.isToken(lhs.node))
  {
    // is token
    leftToken = lhs.node;
    rightToken = lhs.node;
  }
  else
  {
    leftToken = gsLeftToken->getOutgoingEdges(lhs.node)[0];
    rightToken = gsRightToken->getOutgoingEdges(lhs.node)[0];
  }
  
  
  
  // find each non-token node that is left-aligned with the left token and right aligned with the right token
  auto leftAligned = gsLeftToken->getOutgoingEdges(leftToken);
  
  std::unique_ptr<ListWrapper> w = std::make_unique<ListWrapper> (leftAligned.size()+1);
  
  // add the connected token itself as a match the span covers only one token
  if(leftToken == rightToken)
  {
    w->addMatch({leftToken, anyNodeAnno});
  }
  
  for(const auto& candidate : leftAligned)
  {
    // check if also right aligned
    auto candidateRight = gsRightToken->getOutgoingEdges(candidate)[0];
    if(candidateRight == rightToken)
    {
      w->addMatch({candidate, anyNodeAnno});
    }
  }

  return w;
}



IdenticalCoverage::~IdenticalCoverage()
{
}


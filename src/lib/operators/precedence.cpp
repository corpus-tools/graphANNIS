#include "precedence.h"
#include "defaultjoins.h"

using namespace annis;

Precedence::Precedence(DB &db, AnnotationIterator& left, AnnotationIterator& right,
                       unsigned int minDistance, unsigned int maxDistance)
  : db(db), left(left), right(right), minDistance(minDistance), maxDistance(maxDistance),
    tokIteratorForLeftNode(RightMostTokenForNodeIterator(left, db)),
    annoForRightNode(right.getAnnotation()),
    actualJoin(NULL),
    edbLeft(NULL)
{
  const EdgeDB* edbOrder = db.getEdgeDB(ComponentType::ORDERING, annis_ns, "");
  edbLeft = db.getEdgeDB(ComponentType::LEFT_TOKEN, annis_ns, "");
  if(edbOrder != NULL)
  {
    Annotation anyTokAnno = initAnnotation(db.getTokStringID(), 0, db.getNamespaceStringID());
    // TODO: allow to use a nested loop iterator as a configurable alternative
    actualJoin = new SeedJoin(db, edbOrder, tokIteratorForLeftNode, anyTokAnno, minDistance, maxDistance);
  }
  currentMatchedToken.found = true;
}

Precedence::~Precedence()
{
  delete actualJoin;
}

BinaryMatch Precedence::next()
{
  BinaryMatch result;
  result.found = false;
  if(actualJoin != NULL && edbLeft != NULL)
  {
    while(currentMatches.empty() && currentMatchedToken.found)
    {
      currentMatchedToken = actualJoin->next();
      if(currentMatchedToken.found)
      {
        std::vector<nodeid_t> matchCandidateNodes = edbLeft->getOutgoingEdges(currentMatchedToken.rhs.node);
        // also check the token itself
        matchCandidateNodes.insert(matchCandidateNodes.end(),
                                   currentMatchedToken.rhs.node);

        for(nodeid_t nodeID : matchCandidateNodes)
        {
          for(auto& nodeAnno : db.getNodeAnnotationsByID(nodeID))
          {
            if(checkAnnotationEqual(nodeAnno, annoForRightNode))
            {
              Match m;
              m.node = nodeID;
              m.anno = nodeAnno;
              currentMatches.push_back(m);
            }
          }
        }
      }


    }

    if(!currentMatches.empty())
    {
      result.found = true;
      result.lhs = tokIteratorForLeftNode.currentNodeMatch();
      result.rhs = currentMatches.front();
      currentMatches.pop_front();
      return result;
    }

  }
  return result;
}

void Precedence::reset()
{
  if(actualJoin != NULL)
  {
    actualJoin->reset();
  }
  currentMatches.clear();
  currentMatchedToken.found = true;
}


RightMostTokenForNodeIterator::RightMostTokenForNodeIterator(AnnotationIterator &source, const DB &db)
  : source(source), db(db), edb(db.getEdgeDB(ComponentType::RIGHT_TOKEN, annis_ns, ""))
{
  anyTokAnnotation = initAnnotation(db.getTokStringID(), 0, db.getNamespaceStringID());
}

bool RightMostTokenForNodeIterator::hasNext()
{
  return source.hasNext();
}

Match RightMostTokenForNodeIterator::next()
{
  Match result;
  if(source.hasNext() && edb != NULL)
  {
    currentOriginalMatch = source.next();

    // check if this is a token
    std::vector<Annotation> annos = db.getNodeAnnotationsByID(currentOriginalMatch.node);
    for(auto& a : annos)
    {
      if(checkAnnotationEqual(anyTokAnnotation, a))
      {
        return currentOriginalMatch;
      }
    }

    result.node = edb->getOutgoingEdges(currentOriginalMatch.node)[0];
    result.anno.name = db.getTokStringID();
    result.anno.ns = db.getNamespaceStringID();
    result.anno.val = 0; //TODO: do we want to include the actual value here?
  }

  return result;
}

void RightMostTokenForNodeIterator::reset()
{
  source.reset();
}

Match RightMostTokenForNodeIterator::currentNodeMatch()
{

}


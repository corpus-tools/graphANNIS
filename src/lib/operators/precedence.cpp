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
}

Precedence::~Precedence()
{
  delete actualJoin;
}

BinaryMatch Precedence::next()
{
  //TODO: the token itself might match the conditions
  BinaryMatch result;
  result.found = false;
  if(actualJoin != NULL && edbLeft != NULL)
  {
    for(BinaryMatch matchedToken = actualJoin->next(); matchedToken.found; matchedToken = actualJoin->next())
    {
      std::vector<nodeid_t> nodeCandiates = edbLeft->getOutgoingEdges(matchedToken.right.node);
      // first check the token itself
      nodeCandiates.insert(nodeCandiates.begin(), matchedToken.right.node);
      for(auto nodeID : nodeCandiates)
      {
        for(auto& nodeAnno : db.getNodeAnnotationsByID(nodeID))
        {
          if(checkAnnotationEqual(nodeAnno, annoForRightNode))
          {
            result.found = true;
            result.left = tokIteratorForLeftNode.currentNodeMatch();
            result.right.node = nodeID;
            result.right.anno = nodeAnno;
            return result;
          }
        }
      }
    } // end while a matched token was found
  }
  return result;
}

void Precedence::reset()
{
  if(actualJoin != NULL)
  {
    actualJoin->reset();
  }
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


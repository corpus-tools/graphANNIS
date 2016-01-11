#ifndef SEED_H
#define SEED_H

#include "types.h"
#include "iterators.h"
#include "operator.h"
#include "../graphstorage.h"
#include "db.h"

#include <unordered_set>

namespace annis
{

/** A join that takes the left argument as a seed, finds all connected nodes (matching the distance) and checks the condition for each node. */
class AnnoKeySeedJoin : public BinaryIt
{
public:
  AnnoKeySeedJoin(const DB& db, std::shared_ptr<Operator> op,
           std::shared_ptr<AnnoIt> lhs,
           const std::set<AnnotationKey> &rightAnnoKeys);
  virtual ~AnnoKeySeedJoin() {}

  virtual BinaryMatch next();
  virtual void reset();
private:
  const DB& db;
  std::shared_ptr<Operator> op;

  std::shared_ptr<AnnoIt> left;
  const std::set<AnnotationKey>& rightAnnoKeys;
  unsigned int minDistance;
  unsigned int maxDistance;

  std::unique_ptr<AnnoIt> matchesByOperator;
  BinaryMatch currentMatch;
  bool currentMatchValid;
  std::list<Annotation> matchingRightAnnos;

  bool nextLeftMatch();
  bool nextRightAnnotation();

  bool checkReflexitivity(const nodeid_t& lhsNode, const Annotation& lhsAnno, const nodeid_t& rhsNode, const Annotation& rhsAnno)
  {
    if(!op->isReflexive() && lhsNode == rhsNode && checkAnnotationKeyEqual(lhsAnno, rhsAnno))
    {
      return false;
    }
    else
    {
      return true;
    }
  }

};

/**
 * @brief The MaterializedSeedJoin class
 */
class MaterializedSeedJoin : public BinaryIt
{
public:
  MaterializedSeedJoin(const DB& db, std::shared_ptr<Operator> op,
                       std::shared_ptr<AnnoIt> lhs,
                       const std::unordered_set<Annotation> &rightAnno);
  virtual ~MaterializedSeedJoin() {}

  virtual BinaryMatch next();
  virtual void reset();
private:
  const DB& db;
  std::shared_ptr<Operator> op;

  std::shared_ptr<AnnoIt> left;
  const std::unordered_set<Annotation>& right;
  unsigned int minDistance;
  unsigned int maxDistance;

  std::unique_ptr<AnnoIt> matchesByOperator;
  BinaryMatch currentMatch;
  bool currentMatchValid;
  std::list<Annotation> matchingRightAnnos;

  bool nextLeftMatch();
  bool nextRightAnnotation();

  bool checkReflexitivity(const nodeid_t& lhsNode, const Annotation& lhsAnno, const nodeid_t& rhsNode, const Annotation& rhsAnno)
  {
    if(!op->isReflexive() && lhsNode == rhsNode && checkAnnotationKeyEqual(lhsAnno, rhsAnno))
    {
      return false;
    }
    else
    {
      return true;
    }
  }

};



} // end namespace annis

#endif // SEED_H

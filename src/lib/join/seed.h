#ifndef SEED_H
#define SEED_H

#include "types.h"
#include "iterators.h"
#include "operator.h"
#include "edgedb.h"
#include "db.h"

#include <unordered_set>

namespace annis
{

/** A join that takes the left argument as a seed, finds all connected nodes (matching the distance) and checks the condition for each node. */
class SeedJoin : public BinaryIt
{
public:
  SeedJoin(const DB& db, std::shared_ptr<Operator> op,
           std::shared_ptr<AnnoIt> lhs,
           const std::unordered_set<Annotation> &rightAnno,
           const std::set<AnnotationKey> &rightAnnoKeys);
  virtual ~SeedJoin();

  virtual BinaryMatch next();
  virtual void reset();
private:
  const DB& db;
  std::shared_ptr<Operator> op;

  std::shared_ptr<AnnoIt> left;
  const std::unordered_set<Annotation>& right;
  const std::set<AnnotationKey>& rightAnnoKeys;
  unsigned int minDistance;
  unsigned int maxDistance;

  std::unique_ptr<AnnoIt> matchesByOperator;
  BinaryMatch currentMatch;
  bool currentMatchValid;
  std::list<Annotation> matchingRightAnnos;

  bool anyNodeShortcut;

  bool nextLeftMatch();
  bool nextRightAnnotation();

};


} // end namespace annis

#endif // SEED_H

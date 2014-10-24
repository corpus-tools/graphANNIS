#ifndef NESTEDLOOPJOIN_H
#define NESTEDLOOPJOIN_H

#include "types.h"
#include "annotationiterator.h"
#include "edgedb.h"
#include "db.h"

namespace annis
{

/** A join that checks all combinations of the left and right matches if their are connected. */
class NestedLoopJoin : public BinaryOperatorIterator
{
public:
  NestedLoopJoin(const EdgeDB* edb, AnnotationIterator &left, AnnotationIterator &right,
                 unsigned int minDistance = 1, unsigned int maxDistance = 1);
  virtual ~NestedLoopJoin();

  virtual BinaryMatch next();
  virtual void reset();
private:
  const EdgeDB* edb;
  AnnotationIterator& left;
  AnnotationIterator& right;
  unsigned int minDistance;
  unsigned int maxDistance;

  Match matchLeft;
  Match matchRight;

};

/** A join that takes the left argument as a seed, finds all connected nodes (matching the distance) and checks the condition for each node. */
class SeedJoin : public BinaryOperatorIterator
{
public:
  SeedJoin(const DB& db, const EdgeDB* edb, AnnotationIterator &left, const Annotation &right,
                 unsigned int minDistance = 1, unsigned int maxDistance = 1);
  virtual ~SeedJoin();

  virtual BinaryMatch next();
  virtual void reset();
private:
  const DB& db;
  const EdgeDB* edb;
  AnnotationIterator& left;
  const Annotation& right;
  unsigned int minDistance;
  unsigned int maxDistance;

  Match matchLeft;

};

} // end namespace annis

#endif // NESTEDLOOPJOIN_H

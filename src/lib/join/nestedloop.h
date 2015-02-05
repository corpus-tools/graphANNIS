#ifndef NESTEDLOOP_H
#define NESTEDLOOP_H

#include "types.h"
#include "iterators.h"
#include "operator.h"
#include "../graphstorage.h"
#include "db.h"

namespace annis
{

/** A join that checks all combinations of the left and right matches if their are connected. */
class NestedLoopJoin : public BinaryIt
{
public:
  NestedLoopJoin(std::shared_ptr<Operator> op,
                 std::shared_ptr<AnnoIt> lhs, std::shared_ptr<AnnoIt> rhs);
  virtual ~NestedLoopJoin();

  virtual BinaryMatch next();
  virtual void reset();
private:
  std::shared_ptr<Operator> op;
  bool initialized;

  std::shared_ptr<AnnoIt> left;
  std::shared_ptr<AnnoIt> right;

  Match matchLeft;
  Match matchRight;

};


} // end namespace annis

#endif // NESTEDLOOP_H

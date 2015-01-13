#ifndef QUERY_H
#define QUERY_H

#include <memory>
#include <vector>
#include <list>
#include <set>

#include "db.h"
#include <iterators.h>
#include "operator.h"
#include "wrapper.h"

namespace annis
{

struct OperatorEntry
{
  std::shared_ptr<Operator> op;
  size_t idxLeft;
  size_t idxRight;
  bool useNestedLoop;
};

class Query
{
public:
  Query(const DB& db);

  /**
   * @brief Add a new node to query
   * @param n The initial source
   * @return new node number
   */
  size_t addNode(std::shared_ptr<AnnoIt> n);

  /**
   * @brief add an operator to the execution queue
   * @param op
   * @param idxLeft index of LHS node
   * @param idxRight index of RHS node
   * @param useNestedLoop if true a nested loop join is used instead of the default "seed join"
   */
  void addOperator(std::shared_ptr<Operator> op, size_t idxLeft, size_t idxRight, bool useNestedLoop = false);

  bool hasNext();
  std::vector<Match> next();

private:

  const DB& db;

  std::vector<std::shared_ptr<AnnoIt>> source;
  std::list<std::shared_ptr<AnnoIt>> nodes;
  std::list<OperatorEntry> operators;

  bool initialized;

  std::map<int, int> querynode2component;

  void internalInit();

  void addJoin(OperatorEntry &e, bool filterOnly = false);

  void mergeComponents(int c1, int c2);

};

} // end namespace annis
#endif // QUERY_H

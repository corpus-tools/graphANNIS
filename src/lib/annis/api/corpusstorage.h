#pragma once

#include <memory>
#include <vector>
#include <list>

#include <annis/db.h>
#include <annis/dbcache.h>
#include <annis/json/jsonqueryparser.h>

#include <annis/api/graphupdate.h>

namespace annis
{
namespace api
{
/**
 * An API for managing corpora stored in a common location on the file system.
 */
class CorpusStorage
{
public:

  struct CountResult
  {
    long long matchCount;
    long long documentCount;
  };

  CorpusStorage(std::string databaseDir);
   ~CorpusStorage();

  /**
   * Count all occurrences of an AQL query in a single corpus.
   *
   * @param corpus
   * @param queryAsJSON
   * @return
   */
  long long count(std::vector<std::string> corpora,
                  std::string queryAsJSON);


  /**
   * Count all occurrences of an AQL query in a single corpus.
   *
   * @param corpus
   * @param queryAsJSON
   * @return
   */
  CountResult countExtra(std::vector<std::string> corpora,
                  std::string queryAsJSON);


  /**
   * Find occurrences of an AQL query in a single corpus.
   * @param corpora
   * @param queryAsJSON
   * @param offset
   * @param limit
   * @return
   */
  std::vector<std::string> find(std::vector< std::string > corpora, std::string queryAsJSON, long long offset=0,
                                long long limit=10);

  void applyUpdate(std::string corpus, GraphUpdate &update);

private:
  const std::string databaseDir;
  std::unique_ptr<DBCache> cache;
};

}} // end namespace annis
